use crate::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    // The identifier differs between dev and stable builds, so their startup entries stay isolated.
    let autostart_app_name = context.config().identifier.clone();
    tauri::Builder::default()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name(autostart_app_name)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let paths = AppPaths::resolve(app.handle()).map_err(std::io::Error::other)?;
            let (task_store, task_storage_error) = match TaskStore::open(paths.database_path()) {
                Ok(store) => (store, None),
                Err(error) => (
                    TaskStore::unavailable(redact_error_text(&error)),
                    Some(redact_error_text(&error)),
                ),
            };
            let event_app = app.handle().clone();
            let task_event_sink = Arc::new(move |event: TaskEvent| {
                event_app
                    .emit("task-updated", event)
                    .map_err(|error| error.to_string())
            });
            let workspace_event_app = app.handle().clone();
            let workspace_event_sink = Arc::new(move |event_name, payload| {
                workspace_event_app
                    .emit(event_name, payload)
                    .map_err(|error| error.to_string())
            });
            app.manage(AppState::new_with_runtime(
                paths,
                task_store,
                task_storage_error,
                Some(task_event_sink),
                Some(workspace_event_sink),
            ));
            setup_window_chrome(app.handle())?;
            setup_app_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            handle_window_close_request(window, event);
            handle_workspace_native_drop(window, event);
        })
        .invoke_handler(tauri::generate_handler![
            app_health,
            get_app_update_status,
            check_stable_update,
            install_stable_update,
            get_settings,
            save_settings,
            set_launch_at_login,
            detect_network_proxy,
            choose_close_button_behavior,
            get_ssh_status,
            generate_ed25519_key,
            list_ssh_config_hosts,
            upsert_ssh_config_host,
            delete_ssh_config_host,
            list_hosts,
            refresh_discovered_hosts,
            add_host,
            update_host,
            delete_host,
            test_ssh_connection,
            ssh_check,
            bootstrap_ssh_host,
            bootstrap_existing_ssh_host,
            remote_probe_codex,
            batch_remote_probe_codex,
            sample_host_resources,
            remote_manage_codex,
            preview_batch_remote_codex_update,
            batch_remote_update_codex,
            refresh_latest_codex_version,
            get_local_codex_status,
            list_profiles,
            create_profile,
            update_profile,
            delete_profile,
            duplicate_profile,
            import_profiles,
            set_profile_api_key,
            get_profile_api_key,
            delete_profile_api_key,
            preview_profile_apply,
            apply_profile,
            detect_cc_switch_profiles,
            import_cc_switch_profiles,
            list_local_skills,
            import_local_skill,
            update_library_skill_about,
            get_skill_inventory_status,
            detect_installed_skills,
            download_github_skill,
            get_skill_targets,
            install_skill_targets,
            uninstall_skill_targets,
            delete_library_skill,
            download_installed_skill,
            uninstall_installed_skill,
            list_tasks,
            query_tasks,
            get_task,
            acknowledge_task,
            clear_task_history,
            record_frontend_error,
            get_storage_health,
            preview_storage_migration,
            apply_storage_migration,
            preview_storage_restore,
            restore_storage_backup,
            list_skill_packs,
            workspace_open_terminal,
            workspace_list_terminal_sessions,
            workspace_attach_terminal,
            workspace_terminal_write,
            workspace_terminal_resize,
            workspace_terminal_ack,
            workspace_reconnect_terminal,
            workspace_close_terminal,
            workspace_list_local_roots,
            workspace_open_files,
            workspace_open_folder_in_vscode,
            workspace_list_directory,
            workspace_start_file_search,
            workspace_cancel_file_search,
            workspace_preview_file,
            workspace_create_directory,
            workspace_copy_file_entry,
            workspace_copy_file_entries,
            workspace_move_file_entries,
            workspace_save_text_file,
            workspace_validate_terminal_cwd,
            workspace_close_files,
            workspace_prepare_file_operation,
            workspace_confirm_file_operation,
            workspace_restore_recovery,
            workspace_prepare_recovery_purge,
            workspace_purge_recovery,
            workspace_restore_local_transfer_recovery,
            workspace_prepare_local_transfer_recovery_purge,
            workspace_purge_local_transfer_recovery,
            workspace_enqueue_transfers,
            workspace_list_transfers,
            workspace_pause_transfer,
            workspace_resume_transfer,
            workspace_cancel_transfer,
            workspace_retry_transfer,
            workspace_resolve_transfer_conflict,
            workspace_select_upload_sources,
            workspace_select_download_target
        ])
        .build(context)
        .expect("error while building CodexHub")
        .run({
            // Window hide/minimize keeps Workspace sessions alive. Only a true
            // application exit closes PTYs and SFTP subprocesses.
            let shutdown_once = Arc::new(std::sync::atomic::AtomicBool::new(false));
            move |app, event| {
                if matches!(
                    event,
                    tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
                ) && !shutdown_once.swap(true, std::sync::atomic::Ordering::AcqRel)
                {
                    let state = app.state::<AppState>();
                    if let Ok(workspace) = state.workspace.as_ref() {
                        if let Err(error) = tauri::async_runtime::block_on(workspace.shutdown()) {
                            eprintln!(
                                "Could not clean up Workspace resources: {}",
                                redact_error_text(&error.to_string())
                            );
                        }
                    }
                }
            }
        });
}

fn setup_window_chrome(app: &AppHandle) -> tauri::Result<()> {
    #[cfg(windows)]
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.set_decorations(false)?;
    }

    Ok(())
}

fn setup_app_tray(app: &AppHandle) -> tauri::Result<()> {
    let app_name = app_display_name(app);
    let menu = MenuBuilder::new(app)
        .text(TRAY_MENU_SHOW_ID, format!("Show {app_name}"))
        .separator()
        .text(TRAY_MENU_QUIT_ID, format!("Quit {app_name}"))
        .build()?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip(&app_name)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_MENU_SHOW_ID => show_main_window(app),
            TRAY_MENU_QUIT_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)?;
    Ok(())
}

fn handle_window_close_request(window: &Window, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    if window.label() != MAIN_WINDOW_LABEL {
        return;
    }

    api.prevent_close();
    let app = window.app_handle();
    let state = app.state::<AppState>();
    match read_settings(&state.paths)
        .unwrap_or_default()
        .close_button_behavior
    {
        CloseButtonBehavior::Ask => {
            if let Err(error) = app.emit(CLOSE_BUTTON_BEHAVIOR_REQUESTED_EVENT, ()) {
                eprintln!(
                    "Could not emit close preference request: {}",
                    redact_error_text(&error.to_string())
                );
            }
        }
        CloseButtonBehavior::Exit => app.exit(0),
        CloseButtonBehavior::MinimizeToTray => {
            log_best_effort("hide application window", window.hide());
        }
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceLocalDragState<'a> {
    phase: &'a str,
    client_x: Option<f64>,
    client_y: Option<f64>,
}

/// Native drag movement exposes only webview-relative coordinates. Raw paths
/// stay in Rust and are converted to opaque one-shot grants after the drop.
fn handle_workspace_native_drop(window: &Window, event: &WindowEvent) {
    if window.label() != MAIN_WINDOW_LABEL {
        return;
    }
    let WindowEvent::DragDrop(drag_event) = event else {
        return;
    };

    let scale_factor = window.scale_factor().unwrap_or(1.0).max(f64::EPSILON);
    let emit_position = |phase, position: &tauri::PhysicalPosition<f64>| {
        let _ = window.app_handle().emit(
            "workspace-local-drag-state",
            WorkspaceLocalDragState {
                phase,
                client_x: Some(position.x / scale_factor),
                client_y: Some(position.y / scale_factor),
            },
        );
    };

    let paths = match drag_event {
        tauri::DragDropEvent::Enter { position, .. } => {
            emit_position("enter", position);
            return;
        }
        tauri::DragDropEvent::Over { position } => {
            emit_position("over", position);
            return;
        }
        tauri::DragDropEvent::Leave => {
            let _ = window.app_handle().emit(
                "workspace-local-drag-state",
                WorkspaceLocalDragState {
                    phase: "leave",
                    client_x: None,
                    client_y: None,
                },
            );
            return;
        }
        tauri::DragDropEvent::Drop { paths, position } => {
            emit_position("drop", position);
            paths.clone()
        }
        _ => return,
    };
    let app = window.app_handle().clone();
    let services = app.state::<AppState>().services.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(workspace) = services.workspace.as_ref() else {
            return;
        };
        let mut grants = Vec::new();
        for path in paths {
            if let Ok(grant) = workspace
                .transfer_io
                .local_grants
                .grant_upload_file(path)
                .await
            {
                grants.push(grant);
            }
        }
        if !grants.is_empty() {
            let _ = app.emit("workspace-local-drop", grants);
        }
    });
}

pub(crate) fn app_display_name(app: &AppHandle) -> String {
    app.get_webview_window(MAIN_WINDOW_LABEL)
        .and_then(|window| window.title().ok())
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "CodexHub".into())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        log_best_effort("show application window", window.show());
        log_best_effort("restore application window", window.unminimize());
        log_best_effort("focus application window", window.set_focus());
    }
}

pub(crate) fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        log_best_effort("hide application window", window.hide());
    }
}
