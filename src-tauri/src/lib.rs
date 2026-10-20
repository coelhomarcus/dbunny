mod assistant;
mod commands;
mod db;
mod error;
mod menu;

use commands::state::AppState;
use commands::storage;
use tauri::Manager;

#[cfg(debug_assertions)]
#[allow(dead_code)]
#[tauri::command]
fn open_devtools(window: tauri::WebviewWindow) {
    window.open_devtools();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(|app| menu::build(app, "en"))
        .on_menu_event(|app, event| {
            use tauri::Emitter;
            if event.id() == menu::CLOSE_TAB_ID {
                let _ = app.emit(menu::CLOSE_TAB_EVENT, ());
            } else if event.id() == menu::CLOSE_WINDOW_ID {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.close();
                }
            } else if event.id() == menu::SETTINGS_ID || event.id() == menu::CHECK_UPDATES_ID {
                let _ = app.emit(&format!("menu:{}", event.id().as_ref()), ());
            }
        });

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // The app's own key, kept with its data; the hostname key only opens older files
            let legacy = storage::hostname_key();
            let key = match app.path().app_data_dir() {
                Ok(dir) => storage::load_or_create_key(&dir).unwrap_or_else(|e| {
                    eprintln!("{e}; using the hostname key");
                    legacy
                }),
                Err(_) => legacy,
            };
            // Updates come from the GitHub releases (see plugins.updater in tauri.conf.json)
            #[cfg(desktop)]
            app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;

            let state = AppState::new(key);
            if let Ok(path) = storage::get_storage_path(app.handle()) {
                let mut connections = storage::load_connections_from_path(&path);
                if storage::migrate_to_key(&mut connections, &key, &legacy) {
                    if let Err(e) = storage::write_connections(&path, &connections) {
                        eprintln!("Couldn't save the connections under the new key: {e}");
                    }
                }
                *state.saved_connections.blocking_lock() = connections;
            }
            let data_dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("dbunny"));
            let events = std::sync::Arc::new(commands::assistant::TauriEvents(app.handle().clone()));
            app.manage(assistant::Assistant::new(data_dir, state.sessions.clone(), events));
            app.manage(state);

            // Windows and Linux: the title bar (tabs and window controls) is drawn by the app
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_decorations(false);
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            menu::set_menu_language,
            commands::connect::connect,
            commands::connect::connect_sqlite,
            commands::connect::connect_mysql,
            commands::connect::connect_mariadb,
            commands::connect::disconnect,
            commands::catalog::get_function_detail,
            commands::catalog::get_columns,
            commands::catalog::get_relation_kind,
            commands::catalog::get_table_structure,
            commands::catalog::get_extensions,
            commands::catalog::get_object_definition,
            commands::catalog::get_explorer,
            commands::catalog::get_diagram,
            commands::schema_edit::rename_relation,
            commands::schema_edit::drop_relation,
            commands::schema_edit::truncate_table,
            commands::schema_edit::refresh_materialized_view,
            commands::schema_edit::duplicate_table,
            commands::admin::create_schema,
            commands::admin::rename_schema,
            commands::admin::drop_schema,
            commands::admin::create_database,
            commands::admin::drop_database,
            commands::admin::get_server_activity,
            commands::admin::signal_backend,
            commands::admin::connect_database,
            commands::admin::get_databases,
            commands::data::import_rows,
            commands::schema_edit::preview_structure_change,
            commands::schema_edit::apply_structure_change,
            commands::schema_edit::preview_create_table,
            commands::schema_edit::create_table,
            commands::admin::get_roles,
            commands::console::cancel_query,
            commands::transactions::begin_transaction,
            commands::transactions::commit_transaction,
            commands::transactions::rollback_transaction,
            commands::transactions::transaction_open,
            commands::console::explain_query,
            commands::catalog::get_completion_schema,
            commands::console::execute_query,
            commands::data::get_table_data,
            commands::data::count_table_rows,
            commands::data::insert_row,
            commands::data::update_rows,
            commands::data::delete_rows,
            commands::saved::get_saved_connections,
            commands::saved::save_connection,
            commands::saved::delete_saved_connection,
            commands::assistant::assistant_status,
            commands::assistant::assistant_start,
            commands::assistant::assistant_restart,
            commands::assistant::assistant_new_conversation,
            commands::assistant::assistant_list_conversations,
            commands::assistant::assistant_load_conversation,
            commands::assistant::assistant_prompt,
            commands::assistant::assistant_cancel,
            commands::assistant::assistant_approve,
            commands::assistant::assistant_set_model,
            commands::assistant::assistant_close_conversation,
            commands::assistant::assistant_log,
            commands::assistant::assistant_sign_out,
            commands::assistant::assistant_authenticate,
            #[cfg(debug_assertions)]
            open_devtools,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // The agent is a process of its own: it goes when the app does
            if let tauri::RunEvent::Exit = event {
                let assistant = app.state::<assistant::Assistant>();
                tauri::async_runtime::block_on(assistant.shutdown());
            }
        });
}
