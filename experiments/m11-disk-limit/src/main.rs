//! `m11-disk-limit walk <dir>`: time a size walk (st_blocks) of a tree.
//! `m11-disk-limit race <dir> <limit_mb> <interval_ms> -- <writer argv...>`:
//! start the writer in <dir>, measure <dir> every interval, kill the writer's
//! process group when over the limit, report how far over it got.
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::time::{Duration, Instant};

/// Bytes on disk below `dir` (allocated blocks, so sparse files count as
/// what they use), and the number of entries. Does not follow symlinks.
fn usage(dir: &Path) -> (u64, u64) {
    let (mut bytes, mut n) = (0u64, 0u64);
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue }; // symlink_metadata
            n += 1;
            bytes += m.blocks() * 512;
            if m.is_dir() {
                stack.push(e.path());
            }
        }
    }
    (bytes, n)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a[1].as_str() {
        "walk" => {
            for _ in 0..3 {
                let t = Instant::now();
                let (b, n) = usage(Path::new(&a[2]));
                println!("walk: {n} entries, {:.1} MB, {:?}", b as f64 / 1e6, t.elapsed());
            }
        }
        "race" => {
            let dir = Path::new(&a[2]);
            let limit: u64 = a[3].parse::<u64>().unwrap() * 1024 * 1024;
            let interval = Duration::from_millis(a[4].parse().unwrap());
            let argv = &a[6..];
            let mut child = std::process::Command::new(&argv[0])
                .args(&argv[1..])
                .current_dir(dir)
                .process_group(0)
                .spawn()
                .unwrap();
            let start = Instant::now();
            let (mut walks, mut walk_time) = (0u32, Duration::ZERO);
            loop {
                std::thread::sleep(interval);
                let t = Instant::now();
                let (b, _) = usage(dir);
                walks += 1;
                walk_time += t.elapsed();
                if b > limit {
                    // SAFETY: kill(2) on the writer's process group.
                    unsafe { libc_kill(-(child.id() as i32), 9) };
                    let _ = child.wait();
                    let (after, _) = usage(dir);
                    println!(
                        "over at {:?}: measured {:.0} MB, final {:.0} MB, limit {} MB, overshoot {:.0} MB; {walks} walks, mean {:?}",
                        start.elapsed(), b as f64 / 1048576.0, after as f64 / 1048576.0, limit / 1048576,
                        (after.saturating_sub(limit)) as f64 / 1048576.0, walk_time / walks
                    );
                    return;
                }
                if let Ok(Some(s)) = child.try_wait() {
                    println!("writer ended ({s}) under the limit: {:.0} MB", b as f64 / 1048576.0);
                    return;
                }
            }
        }
        _ => panic!("usage"),
    }
}

extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}
