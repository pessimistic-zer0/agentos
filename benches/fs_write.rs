//! Benchmark 2 of 3: one `fs.write` of 4 KB through the gateway, including
//! the fsynced log records and the checkpoint, against writing the same file
//! directly.
//!
//! Run: `make bench` (needs a delegated cgroup, because building a gateway
//! runs the sandbox startup checks).
//!
//! Besides criterion's own report, the raw time of every sample is written to
//! `$AGENTOS_BENCH_OUT/fs_write-*.txt` (one number per line, in microseconds)
//! so percentiles can be computed from the raw data.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use agentos::agent::scripted::{Script, ScriptedAgent};
use agentos::runtime::{self, RunOptions};
use agentos::tools::{ToolCall, ToolOutcome};
use criterion::{criterion_group, criterion_main, Criterion};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn content(i: u64) -> String {
    // 4096 bytes that differ on every call, like real edits do.
    let mut s = format!("# revision {i}\n");
    while s.len() < 4096 {
        s.push_str("value = 12345678901234567890123456789012345678901234567890\n");
    }
    s.truncate(4096);
    s
}

/// The fair baseline: the same durable replace the gateway does (temp file,
/// fsync, rename), without log or checkpoint.
fn direct_durable(path: &Path, text: &str) {
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::File::create(&tmp).unwrap();
    f.write_all(text.as_bytes()).unwrap();
    f.sync_all().unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

fn dump(name: &str, samples: &[f64]) {
    if let Ok(dir) = std::env::var("AGENTOS_BENCH_OUT") {
        let text: String = samples.iter().map(|s| format!("{s:.1}\n")).collect();
        std::fs::write(Path::new(&dir).join(format!("fs_write-{name}.txt")), text).unwrap();
    }
}

fn bench(c: &mut Criterion) {
    let scratch = root().join("target/bench-fs-write").join(std::process::id().to_string());
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let preset = std::env::var("AGENTOS_TEST_PRESET").unwrap_or_else(|_| "nix".into());
    let opts = RunOptions {
        state_dir: scratch.join("state"),
        repo: root().join("examples/demo-repo"),
        policy: root().join("policies").join(format!("{preset}.toml")),
        verbose: false,
    };
    let script: Script = toml::from_str("task = \"bench\"\n[[step]]\ntool = \"fs.list\"\npath = \".\"\n").unwrap();
    let (mut gw, _lock) = runtime::create(&opts, &ScriptedAgent::new(script)).expect("creating an execution");
    let direct_dir = scratch.join("direct");
    std::fs::create_dir_all(&direct_dir).unwrap();
    let direct_file = direct_dir.join("bench.py");

    let mut i = 0u64;
    let mut agentos_samples = Vec::new();
    c.bench_function("fs.write 4 KB through AgentOS (intent fsync + checkpoint)", |b| {
        b.iter(|| {
            i += 1;
            let t = Instant::now();
            let r = gw.handle(ToolCall::FsWrite { path: "src/bench.py".into(), content: content(i), encoding: None, executable: None }).unwrap();
            agentos_samples.push(t.elapsed().as_secs_f64() * 1e6);
            assert_eq!(r.outcome, ToolOutcome::Ok);
        })
    });
    let mut durable_samples = Vec::new();
    c.bench_function("direct write 4 KB (temp file + fsync + rename)", |b| {
        b.iter(|| {
            i += 1;
            let t = Instant::now();
            direct_durable(&direct_file, &content(i));
            durable_samples.push(t.elapsed().as_secs_f64() * 1e6);
        })
    });
    let mut plain_samples = Vec::new();
    c.bench_function("direct write 4 KB (plain write, no fsync)", |b| {
        b.iter(|| {
            i += 1;
            let t = Instant::now();
            std::fs::write(&direct_file, content(i)).unwrap();
            plain_samples.push(t.elapsed().as_secs_f64() * 1e6);
        })
    });
    dump("agentos", &agentos_samples);
    dump("direct-durable", &durable_samples);
    dump("direct-plain", &plain_samples);
    drop(gw);
    let _ = std::fs::remove_dir_all(&scratch);
}

criterion_group! {
    name = benches;
    // Each AgentOS sample makes a checkpoint folder; keep the count modest.
    config = Criterion::default().sample_size(50).measurement_time(std::time::Duration::from_secs(20)).warm_up_time(std::time::Duration::from_secs(2));
    targets = bench
}
criterion_main!(benches);
