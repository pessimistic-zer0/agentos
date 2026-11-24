//! Content-addressed blob store: each blob is a file named by the SHA-256 of
//! its bytes. The event log stores only the hash (ADR 0004); the bytes live
//! here, outside the workspace, where the sandbox cannot see them.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

/// `sha256:<64 hex digits>` of the given bytes.
pub fn content_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(7 + 64);
    s.push_str("sha256:");
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub struct BlobStore {
    dir: PathBuf,
}

impl BlobStore {
    pub fn new(dir: &Path) -> BlobStore {
        BlobStore { dir: dir.to_path_buf() }
    }

    fn path_for(&self, hash: &str) -> Result<PathBuf> {
        let Some(hex) = hash.strip_prefix("sha256:") else { bail!("not a sha256 hash: {hash}") };
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("not a sha256 hash: {hash}");
        }
        Ok(self.dir.join(&hex[..2]).join(hex))
    }

    /// Store bytes and return their hash. Storing the same bytes twice is a
    /// no-op. The file is written under a temporary name, fsynced, then
    /// renamed, so a crash never leaves a blob with the wrong content under a
    /// real name.
    pub fn put(&self, bytes: &[u8]) -> Result<String> {
        let hash = content_hash(bytes);
        let path = self.path_for(&hash)?;
        if path.exists() {
            return Ok(hash);
        }
        let parent = path.parent().expect("blob path has a parent");
        std::fs::create_dir_all(parent)?;
        let tmp = parent.join(format!(".tmp-{}-{}", std::process::id(), &hash[7..19]));
        {
            let mut f = std::fs::File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
            f.write_all(bytes)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &path)?;
        Ok(hash)
    }

    pub fn get(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.path_for(hash)?;
        let bytes = std::fs::read(&path).with_context(|| format!("reading blob {hash}"))?;
        if content_hash(&bytes) != hash {
            bail!("blob {hash} is corrupt");
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_hash() {
        // sha256("abc"), from FIPS 180-2.
        assert_eq!(
            content_hash(b"abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn put_get_round_trip() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("norn-blob-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = BlobStore::new(&dir);
        let h = store.put(b"hello")?;
        assert_eq!(store.put(b"hello")?, h);
        assert_eq!(store.get(&h)?, b"hello");
        assert!(store.get("sha256:zz").is_err());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
