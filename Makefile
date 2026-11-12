# Run inside the dev shell: `nix develop -c make <target>` (or with the tools
# from the README installed). Sandbox tests need a delegated cgroup, so every
# test target goes through systemd-run.

SCOPE := systemd-run --user --scope -p Delegate=yes -q
RUNS  ?= 200

.PHONY: test lint eval eval-recovery eval-security

test:            ## unit tests, vertical slice, security suite, quick recovery run
	$(SCOPE) cargo test

lint:
	cargo clippy --all-targets

eval-recovery:   ## fault injection: RUNS kills per crash point and tool
	AGENTOS_RECOVERY_RUNS=$(RUNS) $(SCOPE) cargo test --test recovery -- --nocapture

eval-security:   ## adversarial suite
	$(SCOPE) cargo test --test security

eval: eval-recovery eval-security   ## M5 adds the benchmarks here
