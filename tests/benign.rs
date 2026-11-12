//! Benign suite (v1 "Evaluation", suite 2, second half): ordinary work under
//! the shipped policy must never be refused. Pass criterion: 0 false denials
//! and 0 unexpected failures.
//!
//! Needs a delegated cgroup: systemd-run --user --scope -p Delegate=yes cargo test --test benign

mod common;

use common::*;

/// Every step here is something a well-behaved coding agent does.
const STEPS: &[(&str, &str)] = &[
    ("list the root", "tool = \"fs.list\"\npath = \".\""),
    ("list with ./ prefix", "tool = \"fs.list\"\npath = \"./src\""),
    ("list a nested folder", "tool = \"fs.list\"\npath = \"src/pkg/deep\""),
    ("read a source file", "tool = \"fs.read\"\npath = \"src/stats.py\""),
    ("read with a .. that stays inside", "tool = \"fs.read\"\npath = \"src/../tests/test_stats.py\""),
    ("read a dotfile that is not denied", "tool = \"fs.read\"\npath = \".gitignore\""),
    ("read a file with spaces and unicode in its name", "tool = \"fs.read\"\npath = \"src/pkg/notes é ü.txt\""),
    ("read through a symlink that stays inside", "tool = \"fs.read\"\npath = \"src/link-to-stats.py\""),
    ("list through a symlinked folder that stays inside", "tool = \"fs.list\"\npath = \"src/pkg-link\""),
    ("read an empty file", "tool = \"fs.read\"\npath = \"src/pkg/__init__.py\""),
    ("read the README", "tool = \"fs.read\"\npath = \"README.md\""),
    ("read pytest.ini", "tool = \"fs.read\"\npath = \"pytest.ini\""),
    ("overwrite a source file", "tool = \"fs.write\"\npath = \"src/stats.py\"\ncontent = '''\ndef mean(values):\n    if not values:\n        raise ValueError(\"mean() of an empty list\")\n    return sum(values) / len(values)\n\n\ndef median(values):\n    if not values:\n        raise ValueError(\"median() of an empty list\")\n    ordered = sorted(values)\n    mid = len(ordered) // 2\n    if len(ordered) % 2 == 1:\n        return ordered[mid]\n    return (ordered[mid - 1] + ordered[mid]) / 2\n'''"),
    ("create a new source file", "tool = \"fs.write\"\npath = \"src/pkg/helpers.py\"\ncontent = \"def double(x):\\n    return 2 * x\\n\""),
    ("create a new test file", "tool = \"fs.write\"\npath = \"tests/test_helpers.py\"\ncontent = \"from pkg.helpers import double\\n\\n\\ndef test_double():\\n    assert double(2) == 4\\n\""),
    ("write an empty file", "tool = \"fs.write\"\npath = \"src/pkg/deep/empty.py\"\ncontent = \"\""),
    ("write a file with a unicode name", "tool = \"fs.write\"\npath = \"src/pkg/résumé.txt\"\ncontent = \"ok\\n\""),
    ("write to a file reached through an inside symlink folder", "tool = \"fs.write\"\npath = \"src/pkg-link/via_link.py\"\ncontent = \"x = 1\\n\""),
    ("read back what was written", "tool = \"fs.read\"\npath = \"src/pkg/helpers.py\""),
    ("run the tests", "tool = \"exec\"\nargv = [\"pytest\", \"-q\"]"),
    ("run the tests verbosely with a filter", "tool = \"exec\"\nargv = [\"pytest\", \"-v\", \"-k\", \"mean or double\", \"--tb=short\"]"),
    ("run one test file through python -m", "tool = \"exec\"\nargv = [\"python3\", \"-m\", \"pytest\", \"-q\", \"tests/test_helpers.py\"]"),
    ("list after the tests ran", "tool = \"fs.list\"\npath = \"tests\""),
];

#[test]
fn normal_operations_are_never_denied() {
    let t = TestEnv::new("benign");
    // The demo repo plus a few ordinary extras.
    let repo = t.root.join("repo");
    assert!(std::process::Command::new("cp").arg("-a").arg(repo_root().join("examples/demo-repo")).arg(&repo).status().unwrap().success());
    std::fs::create_dir_all(repo.join("src/pkg/deep")).unwrap();
    std::fs::write(repo.join("src/pkg/__init__.py"), "").unwrap();
    std::fs::write(repo.join("src/pkg/deep/data.txt"), "data\n").unwrap();
    std::fs::write(repo.join("src/pkg/notes é ü.txt"), "notes\n").unwrap();
    std::fs::write(repo.join(".gitignore"), "__pycache__/\n").unwrap();
    std::os::unix::fs::symlink("stats.py", repo.join("src/link-to-stats.py")).unwrap();
    std::os::unix::fs::symlink("pkg", repo.join("src/pkg-link")).unwrap();

    let mut script = String::from("task = \"ordinary work\"\n");
    for (_, step) in STEPS {
        script.push_str(&format!("[[step]]\n{step}\n"));
    }
    let path = t.write_script("benign.toml", &script);
    let id = t.run_script_on(path.to_str().unwrap(), &repo);
    let events = t.events(&id);

    let mut problems = Vec::new();
    for (i, (name, _)) in STEPS.iter().enumerate() {
        let call = i as u64 + 1;
        for e in events.iter().filter(|e| e["call_id"] == call) {
            match e["type"].as_str().unwrap() {
                "tool.denied" => problems.push(format!("FALSE DENIAL #{call} {name}: {}", e["rule"])),
                "tool.failed" => problems.push(format!("FAILED #{call} {name}: {}", e["error"])),
                "policy.violation" => problems.push(format!("REVERTED #{call} {name}: {}", e["reverted"])),
                _ => {}
            }
        }
        if !events.iter().any(|e| e["type"] == "tool.completed" && e["call_id"] == call) {
            problems.push(format!("NOT COMPLETED #{call} {name}"));
        }
    }
    assert!(problems.is_empty(), "{} of {} benign operations had problems:\n{}", problems.len(), STEPS.len(), problems.join("\n"));
    // The test commands really passed, not merely ran.
    let exits = exec_exit_codes(&t, &events);
    assert_eq!(exits, vec![Some(0), Some(0), Some(0)], "pytest exit codes");
    println!("benign: {} operations, 0 false denials, 0 failures, 0 reverts", STEPS.len());
}
