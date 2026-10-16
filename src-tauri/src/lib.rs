mod commands;

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

/// Tauri's default macOS menu, except Cmd+W closes the active connection tab
/// (handled by the frontend) and Close Window moves to Cmd+Shift+W.
#[cfg(target_os = "macos")]
fn build_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{AboutMetadata, MenuBuilder, MenuItemBuilder, SubmenuBuilder};

    let pkg_info = app.package_info();
    let about = AboutMetadata {
        name: Some(pkg_info.name.clone()),
        version: Some(pkg_info.version.to_string()),
        ..Default::default()
    };

    let app_menu = SubmenuBuilder::new(app, &pkg_info.name)
        .about(Some(about))
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
            }
        });

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::new())
        .setup(|app| {
            let state = app.state::<AppState>();
            if let Ok(path) = storage::get_storage_path(app.handle()) {
                let connections = storage::load_connections_from_path(&path);
                let mut cache = state.saved_connections.blocking_lock();
                *cache = connections;
            }

            #[cfg(target_os = "windows")]
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
            commands::schema::get_schemas,
            commands::schema::get_tables,
            commands::schema::get_views,
            commands::schema::get_functions,
            commands::schema::get_function_detail,
            commands::schema::get_columns,
            commands::structure::get_table_structure,
            commands::schema::get_materialized_views,
            commands::schema::get_sequences,
            commands::schema::get_types,
            commands::schema::get_extensions,
            commands::schema::get_object_definition,
            commands::objects::rename_relation,
            commands::objects::drop_relation,
            commands::objects::truncate_table,
            commands::objects::refresh_materialized_view,
            commands::objects::duplicate_table,
            commands::activity::get_server_activity,
            commands::activity::signal_backend,
            commands::connect::connect_database,
            commands::connect::get_databases,
            commands::query::import_rows,
            commands::editor::cancel_query,
            commands::editor::begin_transaction,
            commands::editor::commit_transaction,
            commands::editor::rollback_transaction,
            commands::editor::transaction_open,
            commands::editor::explain_query,
            commands::schema::get_completion_schema,
            commands::query::execute_query,
            commands::query::get_table_data,
            commands::query::count_table_rows,
            commands::query::insert_row,
            commands::query::update_row,
            commands::query::delete_rows,
            commands::saved::get_saved_connections,
            commands::saved::save_connection,
            commands::saved::delete_saved_connection,
            #[cfg(debug_assertions)]
            open_devtools,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
