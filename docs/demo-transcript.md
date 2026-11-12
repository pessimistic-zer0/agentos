# Demo transcript

Output of `nix develop -c make demo` on 2026-11-12 (NixOS 26.05, kernel 7.1.8, `nix` preset). This is the "demo recording": a text transcript, not a video. Local paths are replaced by `<state>` and `<repo>`; nothing else is edited.

```text

== Building agentos

== 1. The demo repo has a failing test (run directly, outside AgentOS, on a copy)
=========================== short test summary info ============================
FAILED tests/test_stats.py::test_mean - assert 3.3333333333333335 == 2.5
1 failed, 3 passed in 0.03s

== 2. A scripted agent fixes it. Every call goes through policy (policies/nix.toml), log and sandbox
20261112-121134206-3a3d57  RUNNING  policy default-nix  agent scripted
  workspace: 4 files copied, left out by deny rules: .env
  #1   fs.list  .                                        allow  4 entries
  #2   exec     pytest -q                                allow  exit 1, 0.2s
  #3   fs.read  tests/test_stats.py                      allow  304 bytes
  #4   fs.read  src/stats.py                             allow  554 bytes
  #5   fs.read  .env                                     DENY   filesystem.deny[0] ".env"
  #6   fs.write src/stats.py (548 bytes)                 allow  wrote 548 bytes
  #7   exec     pytest -q                                allow  exit 0, 0.2s
20261112-121134206-3a3d57  COMPLETED  script finished after 7 steps
  patch: <state>/executions/20261112-121134206-3a3d57/result.patch

== 3. The result is a patch; the original repo is untouched
diff -ruN --no-dereference a/src/stats.py b/src/stats.py
--- a/src/stats.py	2026-11-12 17:41:34.418467358 +0530
+++ b/src/stats.py	2026-11-12 17:41:34.675828634 +0530
@@ -5,7 +5,7 @@
     """Return the arithmetic mean of a non-empty list of numbers."""
     if not values:
         raise ValueError("mean() of an empty list")
-    return sum(values) / (len(values) - 1)
+    return sum(values) / len(values)
 
 
 def median(values):
   .env in the workspace? no: denied files are never copied in

== 4. Same task again, but the runtime is killed (SIGKILL) right after the file write, before its checkpoint
20261112-121134933-8e81e2  RUNNING  policy default-nix  agent scripted
  workspace: 4 files copied, left out by deny rules: .env
  #1   fs.list  .                                        allow  4 entries
  #2   exec     pytest -q                                allow  exit 1, 0.4s
  #3   fs.read  tests/test_stats.py                      allow  304 bytes
  #4   fs.read  src/stats.py                             allow  554 bytes
  #5   fs.read  .env                                     DENY   filesystem.deny[0] ".env"
<repo>/demo.sh: line 38: 1058891 Killed                     AGENTOS_CRASH_AT=P4 AGENTOS_CRASH_CALL=6 $A run --repo examples/demo-repo --policy "$policy" --script examples/scripts/fix-demo.toml
   -> exit code 137 (137 = killed by signal 9)
20261112-121134933-8e81e2  interrupted (run `agentos recover`)

== 5. Recovery: was call #6 applied? The intent stored the file's hash before and after
20261112-121134933-8e81e2  RECOVERED
  #6   fs.write APPLIED: file has the new content; checkpoint cp-0002 taken

== 6. Resume: the agent continues after the recovered call; nothing is done twice
20261112-121134933-8e81e2  no call was cut off; nothing to recover
20261112-121134933-8e81e2  RESUMING after 6 calls
  #7   exec     pytest -q                                allow  exit 0, 0.2s
20261112-121134933-8e81e2  COMPLETED  script finished after 7 steps
  patch: <state>/executions/20261112-121134933-8e81e2/result.patch

== 7. The trace of the crashed-and-resumed run
#1   fs.list  .                                        allow  4 entries
#2   exec     pytest -q                                allow  exit 1, 0.4s  [cp-0001]
#3   fs.read  tests/test_stats.py                      allow  304 bytes
#4   fs.read  src/stats.py                             allow  554 bytes
#5   fs.read  .env                                     DENY   filesystem.deny[0] ".env"
#6   fs.write src/stats.py (548 bytes)                 allow  RECOVERED, APPLIED: file has the new content; checkpoint cp-0002 taken  [cp-0002]
#7   exec     pytest -q                                allow  exit 0, 0.2s  [cp-0003]

finished: completed (script finished after 7 steps)

== Done. State kept in <state> (delete it when finished)
```
