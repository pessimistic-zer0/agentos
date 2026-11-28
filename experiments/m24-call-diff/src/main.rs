//! E5 (v4, M24): how long does the diff of one call take, with the patch
//! code norn already has, between two checkpoints of a kept execution?
//!
//! Usage: m24-call-diff <execution folder> <out folder>
//! For each pair of neighbouring checkpoints (cp-0000 -> cp-0001, ...), runs
//! `norn::checkpoint::write_patch` five times and prints the time and size.

use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let cps_dir = std::path::Path::new(&a[1]).join("checkpoints");
    let out = std::path::Path::new(&a[2]);
    std::fs::create_dir_all(out).unwrap();
    let mut cps = norn::checkpoint::on_disk(&cps_dir).unwrap();
    cps.sort();
    for w in cps.windows(2) {
        let (before, after) = (cps_dir.join(&w[0]), cps_dir.join(&w[1]));
        let patch = out.join(format!("{}-{}.patch", w[0], w[1]));
        let mut times = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            norn::checkpoint::write_patch(&before, &after, &patch, |_| false).unwrap();
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let size = std::fs::metadata(&patch).unwrap().len();
        println!("{} -> {}: median {:.1} ms (min {:.1}, max {:.1}), patch {} bytes", w[0], w[1], times[2], times[0], times[4], size);
    }
}
