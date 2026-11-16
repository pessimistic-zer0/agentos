//! Base64 (RFC 4648, the standard alphabet with `=` padding), for carrying
//! the bytes of a binary file inside a JSON string (D-123). Written here
//! because it is forty lines and the project keeps its dependencies few.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Three bytes become four characters; a last group of one or two bytes is
/// padded with `=`.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The bytes a base64 text stands for. Line breaks and spaces are skipped
/// (models and the `base64` command wrap long output); anything else that
/// is not in the alphabet, a wrong length, padding in the wrong place or
/// left-over bits are errors, so that what is written is exactly what one
/// text can mean.
pub fn decode(text: &str) -> Result<Vec<u8>, &'static str> {
    let symbols: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !symbols.len().is_multiple_of(4) {
        return Err("its length (without spaces and line breaks) is not a multiple of 4");
    }
    let mut out = Vec::with_capacity(symbols.len() / 4 * 3);
    let groups = symbols.len() / 4;
    for (g, group) in symbols.chunks(4).enumerate() {
        let pad = group.iter().rev().take_while(|b| **b == b'=').count();
        if pad > 2 || (pad > 0 && g + 1 != groups) {
            return Err("`=` padding is only allowed at the very end, one or two characters");
        }
        let mut n = 0u32;
        for &b in &group[..4 - pad] {
            let v = match b {
                b'A'..=b'Z' => b - b'A',
                b'a'..=b'z' => b - b'a' + 26,
                b'0'..=b'9' => b - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return Err("it contains a character that is not in the base64 alphabet (A-Z, a-z, 0-9, +, /)"),
            };
            n = n << 6 | v as u32;
        }
        n <<= 6 * pad as u32;
        // Bits that the padding says are unused must be zero.
        if (pad == 1 && n & 0xff != 0) || (pad == 2 && n & 0xffff != 0) {
            return Err("its last group has bits set that belong to no byte");
        }
        out.extend_from_slice(&n.to_be_bytes()[1..4 - pad]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_vectors_of_rfc_4648() {
        for (plain, coded) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(encode(plain.as_bytes()), coded);
            assert_eq!(decode(coded).unwrap(), plain.as_bytes());
        }
    }

    #[test]
    fn every_byte_value_and_every_length_survives_a_round_trip() {
        let all: Vec<u8> = (0..=255u8).collect();
        for len in 0..=all.len() {
            assert_eq!(decode(&encode(&all[..len])).unwrap(), &all[..len], "length {len}");
        }
        assert_eq!(encode(&[0xfb, 0xff, 0xfe]), "+//+");
    }

    #[test]
    fn wrapped_text_is_read_and_damaged_text_is_refused() {
        assert_eq!(decode("Zm9v\nYmFy\r\n").unwrap(), b"foobar");
        assert_eq!(decode(" Zg = = ").unwrap(), b"f");
        for bad in ["Zg", "Zg=", "Z===", "====", "Zm9v=", "Zg==Zm9v", "Zm9$", "Zm9v-_8=", "Zh==", "Zm9="] {
            assert!(decode(bad).is_err(), "{bad}");
        }
    }
}
