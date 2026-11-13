# Security suite results, `fhs` preset

Source: GitHub Actions run <https://github.com/pessimistic-zer0/agentos/actions/runs/43385661105> on 2026-11-13, commit `c3c9502`, runner image `ubuntu-24.04` (Ubuntu 24.04.5 LTS) version 20261101.360.1, rustc 1.100.0, `AGENTOS_TEST_PRESET=fhs`. The lines below are copied from the log of the run's `make test` step, which runs the same test programs as `make eval-security` (`tests/security/main.rs` and `tests/benign.rs`).

```text
test conftest_cannot_read_a_denied_file ... ok
test denied_paths_cannot_be_created_or_read_later ... ok
test exec_cannot_escape_through_proc_root_links ... ok
test exec_cannot_hardlink_host_files_or_create_devices ... ok
test exec_cannot_mount_even_in_a_nested_user_namespace ... ok
test exec_cannot_reach_host_abstract_unix_sockets ... ok
test exec_cannot_read_a_denied_file ... ok
test exec_cannot_read_host_home_or_original_repo ... ok
test exec_cannot_read_or_change_the_event_log_and_checkpoints ... ok
test exec_cannot_replace_a_folder_with_a_symlink_to_escape_the_revert ... ok
test exec_cannot_signal_host_processes ... ok
test exec_changes_outside_write_globs_are_reverted ... ok
test exec_has_no_capabilities_and_cannot_gain_root ... ok
test exec_has_no_network ... ok
test exec_sees_no_host_secrets_in_environment_or_proc ... ok
test exec_tmp_is_private ... ok
test background_processes_die_with_the_command ... ok
test fs_read_absolute_path_is_denied ... ok
test fs_read_dotdot_traversal_is_denied ... ok
test fs_read_of_a_fifo_planted_by_a_command_does_not_hang_the_runtime ... ok
test fs_read_proc_magic_link_is_denied ... ok
test fs_read_through_symlinks_pointing_outside_is_denied ... ok
test fork_bomb_is_stopped_by_pids_max ... ok
test fs_tools_cannot_reach_log_or_checkpoints ... ok
test fs_write_over_a_symlink_does_not_follow_it ... ok
test fs_write_through_symlinked_folder_is_denied ... ok
test memory_exhaustion_is_stopped_by_memory_max ... ok
test output_flood_is_truncated_and_does_not_fill_the_log ... ok
test patch_does_not_follow_symlinks_to_host_files ... ok
test shipped_policy_refuses_shells_interpreters_and_path_tricks ... ok
test infinite_loop_is_killed_at_the_time_limit ... ok
test toctou_symlink_swap_during_writes_and_reads_never_escapes ... ok

test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.43s
```

Benign suite (`tests/benign.rs`; one test that runs the 23 operations and fails on any denial, failure or revert):

```text
test normal_operations_are_never_denied ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.80s
```

32 adversarial cases, 0 escapes; 23 benign operations, 0 false denials. The same step also passed the unit tests (22), the vertical slice (4), the LLM mock tests (2) and the quick recovery tests (3). The run's lint step (`cargo clippy --all-targets -- -D warnings`) passed.

The TOCTOU test's own counts (swaps, calls ending each way) are printed only with `--nocapture`, which `make test` does not use, so they are not recorded for this preset; the test asserts its minimums itself.
