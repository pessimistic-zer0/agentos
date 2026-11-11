//! Does openat2(RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS) keep every path inside
//! a folder, where a plain openat() and a text check do not?
use nix::fcntl::{open, openat, openat2, OFlag, OpenHow, ResolveFlag};
use nix::sys::stat::Mode;
use std::os::unix::fs::symlink;

fn main() {
    let base = std::env::temp_dir().join(format!("openat2-exp-{}", std::process::id()));
    let ws = base.join("ws");
    std::fs::create_dir_all(ws.join("src")).unwrap();
    std::fs::write(ws.join("src/ok.txt"), "inside\n").unwrap();
    std::fs::write(base.join("secret.txt"), "OUTSIDE SECRET\n").unwrap();
    symlink("/etc/passwd", ws.join("abs-link")).unwrap();          // absolute link out
    symlink("../secret.txt", ws.join("rel-link")).unwrap();        // relative link out
    symlink("src/ok.txt", ws.join("good-link")).unwrap();          // link that stays inside
    let dir = open(&ws, OFlag::O_PATH | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC, Mode::empty()).unwrap();
    let how = OpenHow::new()
        .flags(OFlag::O_RDONLY | OFlag::O_CLOEXEC)
        .resolve(ResolveFlag::RESOLVE_BENEATH | ResolveFlag::RESOLVE_NO_MAGICLINKS);
    println!("{:<28} {:<14} {}", "path", "openat", "openat2 BENEATH");
    for p in ["src/ok.txt", "good-link", "src/../src/ok.txt", "../secret.txt", "/etc/passwd",
              "abs-link", "rel-link", "/proc/self/root/etc/passwd"] {
        let plain = match openat(&dir, p, OFlag::O_RDONLY | OFlag::O_CLOEXEC, Mode::empty()) {
            Ok(_) => "opened".to_string(),
            Err(e) => format!("{e:?}"),
        };
        let beneath = match openat2(&dir, p, how) {
            Ok(_) => "opened".to_string(),
            Err(e) => format!("{e:?} ({})", e.desc()),
        };
        println!("{p:<28} {plain:<14} {beneath}");
    }
    std::fs::remove_dir_all(&base).unwrap();
}
