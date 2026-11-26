# Security suite results with the v3 cases, `nix` preset

Command: `make eval-security` on 2026-11-26, the tests of commit `beaf41a` (run on its working tree just before it was committed; no change to `src/` since `1877d20`), NixOS, kernel 7.1.8. It runs the benign case, then the security suite through the scripted agent, then again through `norn mcp` (`NORN_SECURITY_FRONT=mcp`, D-144).

36 cases: the 33 already in the suite and the three added for v3 (marked †). Both front doors: 36 passed, 0 escapes. The benign case: 1 passed.

## Scripted agent

```text
test a_secret_cannot_be_reached_by_the_agent_or_its_commands ... ok †
test background_processes_die_with_the_command ... ok
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
test fork_bomb_is_stopped_by_pids_max ... ok
test fs_read_absolute_path_is_denied ... ok
test fs_read_dotdot_traversal_is_denied ... ok
test fs_read_of_a_fifo_planted_by_a_command_does_not_hang_the_runtime ... ok
test fs_read_proc_magic_link_is_denied ... ok
test fs_read_through_symlinks_pointing_outside_is_denied ... ok
test fs_tools_cannot_reach_log_or_checkpoints ... ok
test fs_write_over_a_symlink_does_not_follow_it ... ok
test fs_write_through_symlinked_folder_is_denied ... ok
test infinite_loop_is_killed_at_the_time_limit ... ok
test memory_exhaustion_is_stopped_by_memory_max ... ok
test output_flood_is_truncated_and_does_not_fill_the_log ... ok
test patch_does_not_follow_symlinks_to_host_files ... ok
test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.23s
test seccomp_refuses_namespaces_mounts_and_kernel_interfaces ... ok
test shipped_policy_refuses_shells_interpreters_and_path_tricks ... ok
test the_agent_cannot_approve_its_own_call ... ok †
test the_network_allow_list_cannot_be_escaped ... ok †
test toctou_symlink_swap_during_writes_and_reads_never_escapes ... ok
```

## Through `norn mcp`

```text
test a_secret_cannot_be_reached_by_the_agent_or_its_commands ... ok †
test background_processes_die_with_the_command ... ok
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
test fork_bomb_is_stopped_by_pids_max ... ok
test fs_read_absolute_path_is_denied ... ok
test fs_read_dotdot_traversal_is_denied ... ok
test fs_read_of_a_fifo_planted_by_a_command_does_not_hang_the_runtime ... ok
test fs_read_proc_magic_link_is_denied ... ok
test fs_read_through_symlinks_pointing_outside_is_denied ... ok
test fs_tools_cannot_reach_log_or_checkpoints ... ok
test fs_write_over_a_symlink_does_not_follow_it ... ok
test fs_write_through_symlinked_folder_is_denied ... ok
test infinite_loop_is_killed_at_the_time_limit ... ok
test memory_exhaustion_is_stopped_by_memory_max ... ok
test output_flood_is_truncated_and_does_not_fill_the_log ... ok
test patch_does_not_follow_symlinks_to_host_files ... ok
test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.27s
test seccomp_refuses_namespaces_mounts_and_kernel_interfaces ... ok
test shipped_policy_refuses_shells_interpreters_and_path_tricks ... ok
test the_agent_cannot_approve_its_own_call ... ok †
test the_network_allow_list_cannot_be_escaped ... ok †
test toctou_symlink_swap_during_writes_and_reads_never_escapes ... ok
```
