# Run inside the dev shell: `nix develop -c make <target>` (or with the tools
# from the README installed). Sandbox tests need a delegated cgroup, so every
# test target goes through systemd-run.

SCOPE := systemd-run --user --scope -p Delegate=yes -q
RUNS  ?= 200

.PHONY: test lint eval eval-recovery eval-security bench

test:            ## everything, with 3 kills per recovery cell (about 30 s)
	$(SCOPE) cargo test

lint:
	cargo clippy --all-targets

eval-recovery:   ## suite 1: fault injection, RUNS kills per crash point and tool
	AGENTOS_RECOVERY_RUNS=$(RUNS) $(SCOPE) cargo test --test recovery -- --nocapture

eval-security:   ## suite 2: adversarial cases (0 escapes) and benign cases (0 false denials)
	$(SCOPE) cargo test --test security --test benign -- --nocapture

bench:           ## suite 3: the three overhead numbers, raw data in docs/results/bench-<preset>/
	bench/run.sh

eval: eval-recovery eval-security bench   ## the three suites of the v1 design, one command
