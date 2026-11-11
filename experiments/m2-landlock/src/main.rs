//! The landlock crate's basic use: allow read+execute under some folders and
//! read+write under one, then see what a child process can still touch.
use landlock::{path_beneath_rules, Access, AccessFs, Ruleset, RulesetAttr, RulesetCreatedAttr, ABI};
use std::process::Command;

fn main() {
    let work = std::env::temp_dir().join(format!("landlock-exp-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let abi = ABI::V5;
    let ro: Vec<String> = std::env::args().skip(1).collect(); // e.g. /nix/store or /usr /lib
    let status = Ruleset::default()
        .handle_access(AccessFs::from_all(abi)).unwrap()
        .create().unwrap()
        .add_rules(path_beneath_rules(&ro, AccessFs::from_read(abi))).unwrap()
        .add_rules(path_beneath_rules([&work], AccessFs::from_all(abi))).unwrap()
        // Without this, spawning a child fails: Command::output() opens
        // /dev/null for the child's stdin, and Landlock refuses it (EACCES).
        .add_rules(path_beneath_rules(["/dev/null"], AccessFs::ReadFile | AccessFs::WriteFile)).unwrap()
        .restrict_self().unwrap();
    println!("landlock status: {:?}, no_new_privs: {}", status.ruleset, status.no_new_privs);
    let tries = [
        format!("open('{}/a','w').write('x'); print('write in work: ok')", work.display()),
        "print('read /etc/passwd:', open('/etc/passwd').read()[:10])".to_string(),
        format!("print('read home:', __import__('os').listdir('{}')[:2])", std::env::var("HOME").unwrap()),
        "open('/tmp/landlock-escape','w'); print('write /tmp: ok')".to_string(),
    ];
    let py = std::env::var("PY").unwrap();
    for t in tries {
        let out = Command::new(&py).arg("-c").arg(&t).output().unwrap();
        let s = String::from_utf8_lossy(if out.status.success() { &out.stdout } else { &out.stderr });
        println!("  {}", s.trim().lines().last().unwrap_or(""));
    }
}
