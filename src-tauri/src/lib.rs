mod commands;
mod db;

use commands::state::AppState;
use commands::storage;
use tauri::Manager;

#[cfg(debug_assertions)]
#[allow(dead_code)]
#[tauri::command]
fn open_devtools(window: tauri::WebviewWindow) {
    window.open_devtools();
}

/// Menu item ids and the event the frontend listens to for Cmd+W.
#[cfg(target_os = "macos")]
const CLOSE_TAB_ID: &str = "close-tab";
#[cfg(target_os = "macos")]
const CLOSE_WINDOW_ID: &str = "close-window";
#[cfg(target_os = "macos")]
const CLOSE_TAB_EVENT: &str = "menu:close-tab";
#[cfg(target_os = "macos")]
const SETTINGS_ID: &str = "settings";
#[cfg(target_os = "macos")]
const CHECK_UPDATES_ID: &str = "check-updates";

/// Tauri's default macOS menu, except Cmd+W closes the active connection tab
/// (handled by the frontend) and Close Window moves to Cmd+Shift+W. The app menu also
/// opens Settings (Cmd+,) and checks for updates; the frontend gets `menu:<id>`.
#[cfg(target_os = "macos")]
fn build_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{AboutMetadata, MenuBuilder, MenuItemBuilder, SubmenuBuilder};

    let pkg_info = app.package_info();
    let about = AboutMetadata {
        name: Some(pkg_info.name.clone()),
        version: Some(pkg_info.version.to_string()),
        ..Default::default()
    };

    let settings = MenuItemBuilder::with_id(SETTINGS_ID, "Settings…").accelerator("CmdOrCtrl+,").build(app)?;
    let check_updates = MenuItemBuilder::with_id(CHECK_UPDATES_ID, "Check for Updates…").build(app)?;
    let app_menu = SubmenuBuilder::new(app, &pkg_info.name)
        .about(Some(about))
        .item(&check_updates)
        .separator()
        .item(&settings)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    let close_tab = MenuItemBuilder::with_id(CLOSE_TAB_ID, "Close Tab")
        .accelerator("CmdOrCtrl+W")
        .build(app)?;
    let close_window = MenuItemBuilder::with_id(CLOSE_WINDOW_ID, "Close Window")
        .accelerator("CmdOrCtrl+Shift+W")
        .build(app)?;
    let file_menu = SubmenuBuilder::new(app, "File")
        .item(&close_tab)
        .item(&close_window)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let view_menu = SubmenuBuilder::new(app, "View").fullscreen().build()?;

    let window_menu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .build()?;

    MenuBuilder::new(app)
        .items(&[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu])
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(build_menu)
        .on_menu_event(|app, event| {
            use tauri::Emitter;
            if event.id() == CLOSE_TAB_ID {
                let _ = app.emit(CLOSE_TAB_EVENT, ());
            } else if event.id() == CLOSE_WINDOW_ID {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.close();
                }
            } else if event.id() == SETTINGS_ID || event.id() == CHECK_UPDATES_ID {
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
            commands::connect::connect,
            commands::connect::disconnect,
            commands::catalog::get_function_detail,
            commands::catalog::get_columns,
            commands::catalog::get_relation_kind,
            commands::catalog::get_table_structure,
            commands::catalog::get_extensions,
            commands::catalog::get_object_definition,
            commands::catalog::get_explorer,
            commands::objects::rename_relation,
            commands::objects::drop_relation,
            commands::objects::truncate_table,
            commands::objects::refresh_materialized_view,
            commands::objects::duplicate_table,
            commands::objects::create_schema,
            commands::objects::rename_schema,
            commands::objects::drop_schema,
            commands::objects::create_database,
            commands::objects::drop_database,
            commands::activity::get_server_activity,
            commands::activity::signal_backend,
            commands::connect::connect_database,
            commands::connect::get_databases,
            commands::data::import_rows,
            commands::alter::preview_structure_change,
            commands::alter::apply_structure_change,
            commands::alter::preview_create_table,
            commands::alter::create_table,
            commands::roles::get_roles,
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
            #[cfg(debug_assertions)]
            open_devtools,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
