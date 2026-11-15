# What the suites missed

*Six bugs in a tested program, and what each one says about testing.*

AgentOS runs a coding agent's tool calls in a Linux sandbox and keeps a log that survives `kill -9`. By the time it was called finished it had three suites: 4,800 deliberate kills with five invariants checked after each, 32 attacks that all had to fail, and a set of ordinary operations that all had to work. Everything passed, on two systems.

Then it was used: by hand, on things the suites did not try, on projects that were not written for it, through its benchmarks, and on the second system again after new features. That found six more bugs. None let anything out of the sandbox. All of them would have met a real user in the first week.

The full notes are in [`bugs.md`](bugs.md) (B-009 to B-014). This is the short version, with the part that is worth remembering.

## 1. Trusted code, untrusted data (B-009)

After every command, the runtime walks the folder the command worked in and copies it. A command left a file whose name was not valid text. The walk failed, and the whole run stopped with an error.

The code that failed was the runtime's own, so its error had been classed as "the runtime broke". But the data it tripped over was chosen by the code in the sandbox. The split between "the tool failed" and "the runtime failed" had been drawn by *where* an error is raised. The right line is *who caused it*.

**The fix:** such a failure now fails the call and rolls the folder back; the agent is told, and the run goes on.

## 2. Deleting is its own permission (B-010)

A command made a folder, put a file in it, and took away all the folder's permission bits. Recovery rolls a workspace back by swapping in a clean copy and deleting the old one. The swap worked. The delete did not: on Linux the owner of a folder cannot empty it without the write and search bits. Every later attempt started by deleting that leftover and failed at the same place. The execution could never be recovered.

While fixing it, a quieter form appeared: a *read-only* folder with one file in it. It copies without complaint, so it ended up inside a checkpoint, and broke a later rollback. No hostile intent needed.

The security suite asked "did anything get out?". The recovery suite asked "is the state right after a kill?", always with a well-behaved workspace. Nobody asked "can it still be recovered after hostile code ran?".

**The fix:** before deleting a tree that sandboxed code shaped, give its folders their owner bits back.

## 3. A path is half a fact (B-011)

With `--state-dir state`, a relative path, every run failed at its very last step. The patch is made by starting `diff` in another folder, and the two paths handed to it were relative to where agentos had been started.

Every test passed an absolute path. The default is absolute. The first person to type the natural thing would have hit it.

**The fix:** make the path absolute once, where it enters the program.

## 4. A list written from memory (B-012)

A patch is text, so it cannot carry a binary file, a file's mode, an empty folder or a symlink. That list had been written down, and the patch was taught to say so. On the first real project, the agent added a Python package. After applying the patch the package's empty `__init__.py` was missing: `diff` prints nothing for a new file with nothing in it, and `patch` creates nothing from nothing.

The list was one item short because it came from thinking, not from trying each kind of change. The project that found it was written by a separate agent that had been told not to look at AgentOS.

**The fix:** empty files are on the list; later, `agentos apply` copies all of it across.

## 5. A library that thought it was a program (B-013)

To learn whether user namespaces work, the startup check started "the program I am" a second time with a hidden argument. That is fine when the program is `agentos`. The benchmarks are other programs that use the same code. For them the check started a second benchmark, saw it fail, and reported that user namespaces do not work.

It had passed CI on both systems, twice. Nothing in the test suite uses the library from another program; only the benchmarks do, and they are not part of the tests. A script that hid a failed step turned the clear error into a missing file three steps later.

**The fix:** the probe runs in a forked child of whatever program it is in, and never starts anything.

## 6. The machine you do not have (B-014)

A note was added for a common mistake: a virtualenv that is made visible to the sandbox but not activated. Its test was "a visible folder has its own `pytest`, and a different one is in use". On Ubuntu, `/usr` is a visible folder and `/usr/bin/pytest` exists. So every Ubuntu user with an *active* virtualenv got the note, telling them to do what they had done.

On the development machine (NixOS) no such folder exists, so it could not happen there. CI caught it on the first run. And because the test run stops at the first failing file, that red run also said nothing about seven other suites.

**The fix:** "not activated" means "its folder is not on `PATH`"; test that. And a test that builds the Ubuntu shape out of two folders, so it fails anywhere.

## What they have in common

| Bug | Found by | Why the suites could not see it |
|---|---|---|
| B-009, B-010 | Trying odd leftovers by hand | Each suite kept the other's assumption: attacks were never followed by recovery, kills never came with a hostile workspace |
| B-011 | Looking at output with a relative path | Every test used an absolute one |
| B-012 | A project written by someone else | The cases came from the author's own list |
| B-013 | Running the benchmarks | No test used the code from outside the main program |
| B-014 | CI on the other system | The development machine cannot have that layout |

Three things, then:

1. **A suite tests what its author thought of.** The cheapest way to find the rest is to let someone else choose the input: another person, another project, another machine.
2. **Two suites can each be thorough and leave the product between them untested.** The interesting bugs were where security met recovery.
3. **A fix is a change.** B-013 and B-014 were both introduced by features added to help users, and both passed every local test.

None of this argues against the suites. They are why these six were the only ones.
