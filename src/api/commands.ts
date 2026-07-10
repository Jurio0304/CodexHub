export type CommandEffect = "read" | "refresh" | "local-write" | "remote-write";

export type CommandPolicy = {
  effect: CommandEffect;
  liveSsh: boolean;
  sensitiveInput: boolean;
};

// Keep this registry aligned with Rust's `generate_handler!` command surface.
export const commandPolicies = {
  app_health: { effect: "read", liveSsh: false, sensitiveInput: false },
  get_app_update_status: { effect: "read", liveSsh: false, sensitiveInput: false },
  check_stable_update: { effect: "refresh", liveSsh: false, sensitiveInput: false },
  install_stable_update: { effect: "remote-write", liveSsh: false, sensitiveInput: false },
  get_settings: { effect: "read", liveSsh: false, sensitiveInput: false },
  save_settings: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  detect_network_proxy: { effect: "read", liveSsh: false, sensitiveInput: false },
  choose_close_button_behavior: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  get_ssh_status: { effect: "read", liveSsh: false, sensitiveInput: false },
  generate_ed25519_key: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  list_ssh_config_hosts: { effect: "read", liveSsh: false, sensitiveInput: false },
  upsert_ssh_config_host: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  delete_ssh_config_host: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  list_hosts: { effect: "refresh", liveSsh: false, sensitiveInput: false },
  refresh_discovered_hosts: { effect: "refresh", liveSsh: false, sensitiveInput: false },
  add_host: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  update_host: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  delete_host: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  test_ssh_connection: { effect: "refresh", liveSsh: true, sensitiveInput: false },
  ssh_check: { effect: "refresh", liveSsh: true, sensitiveInput: false },
  bootstrap_ssh_host: { effect: "remote-write", liveSsh: true, sensitiveInput: true },
  bootstrap_existing_ssh_host: { effect: "remote-write", liveSsh: true, sensitiveInput: true },
  remote_probe_codex: { effect: "refresh", liveSsh: true, sensitiveInput: false },
  sample_host_resources: { effect: "refresh", liveSsh: true, sensitiveInput: false },
  remote_manage_codex: { effect: "remote-write", liveSsh: true, sensitiveInput: false },
  refresh_latest_codex_version: { effect: "refresh", liveSsh: false, sensitiveInput: false },
  get_local_codex_status: { effect: "read", liveSsh: false, sensitiveInput: false },
  list_profiles: { effect: "read", liveSsh: false, sensitiveInput: false },
  create_profile: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  update_profile: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  delete_profile: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  duplicate_profile: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  import_profiles: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  set_profile_api_key: { effect: "local-write", liveSsh: false, sensitiveInput: true },
  get_profile_api_key: { effect: "read", liveSsh: false, sensitiveInput: true },
  delete_profile_api_key: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  preview_profile_apply: { effect: "read", liveSsh: false, sensitiveInput: false },
  apply_profile: { effect: "remote-write", liveSsh: true, sensitiveInput: false },
  detect_cc_switch_profiles: { effect: "read", liveSsh: false, sensitiveInput: false },
  import_cc_switch_profiles: { effect: "local-write", liveSsh: false, sensitiveInput: true },
  list_local_skills: { effect: "read", liveSsh: false, sensitiveInput: false },
  import_local_skill: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  update_library_skill_about: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  get_skill_inventory_status: { effect: "read", liveSsh: false, sensitiveInput: false },
  detect_installed_skills: { effect: "refresh", liveSsh: true, sensitiveInput: false },
  download_github_skill: { effect: "local-write", liveSsh: false, sensitiveInput: false },
  get_skill_targets: { effect: "read", liveSsh: false, sensitiveInput: false },
  install_skill_targets: { effect: "remote-write", liveSsh: true, sensitiveInput: false },
  uninstall_skill_targets: { effect: "remote-write", liveSsh: true, sensitiveInput: false },
  delete_library_skill: { effect: "remote-write", liveSsh: true, sensitiveInput: false },
  download_installed_skill: { effect: "remote-write", liveSsh: true, sensitiveInput: false },
  uninstall_installed_skill: { effect: "remote-write", liveSsh: true, sensitiveInput: false },
  list_tasks: { effect: "read", liveSsh: false, sensitiveInput: false },
  list_skill_packs: { effect: "read", liveSsh: false, sensitiveInput: false }
} as const satisfies Record<string, CommandPolicy>;

export type TauriCommand = keyof typeof commandPolicies;
