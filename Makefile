# Run inside the dev shell: `nix develop -c make <target>` (or with the tools
# from the README installed). Sandbox tests need a delegated cgroup, so every
# test target goes through systemd-run.

SCOPE := systemd-run --user --scope -p Delegate=yes -q
RUNS  ?= 200

.PHONY: demo test lint eval eval-recovery eval-security bench

demo:            ## fix the demo repo in the sandbox, then crash, recover and resume
	./demo.sh

test:            ## everything, with 3 kills per recovery cell; the security cases a second time through `norn mcp`
	$(SCOPE) cargo test
	NORN_SECURITY_FRONT=mcp $(SCOPE) cargo test --test security

lint:
	cargo clippy --all-targets

eval-recovery:   ## suite 1: fault injection, RUNS kills per crash point and tool, then the external-call cells
	# One test at a time: the two big tests each start 8 workers, and running
	# them together would load the machine twice and blur the recovery times.
	NORN_RECOVERY_RUNS=$(RUNS) $(SCOPE) cargo test --test recovery -- --nocapture --test-threads=1

eval-security:   ## suite 2: adversarial cases (0 escapes) and benign cases (0 false denials)
	$(SCOPE) cargo test --test security --test benign -- --nocapture
	NORN_SECURITY_FRONT=mcp $(SCOPE) cargo test --test security -- --nocapture

bench:           ## suite 3: the three overhead numbers, raw data in docs/results/bench-<preset>/
	bench/run.sh

eval: eval-recovery eval-security bench   ## the three suites of the v1 design, one command
