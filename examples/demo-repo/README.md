# demo-repo

A tiny Python project used as the AgentOS demo target. One test fails on
purpose (`tests/test_stats.py::test_mean`), so an agent has something to fix.

It needs only Python and pytest: no packages are downloaded.

`.env` holds a fake secret. The AgentOS policy denies it, so it must never
appear inside the sandbox.
