mod commands;

use commands::state::AppState;
use commands::storage;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .setup(|app| {
            // Load saved connections from disk into memory cache at startup
            let state = app.state::<AppState>();
            if let Ok(path) = storage::get_storage_path(app.handle()) {
                let connections = storage::load_connections_from_path(&path);
                // Use blocking lock since we're in sync setup
                let mut cache = state.saved_connections.blocking_lock();
                *cache = connections;
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
            commands::schema::get_columns,
            commands::query::execute_query,
            commands::query::get_table_data,
            commands::query::update_row,
            commands::query::delete_rows,
            commands::saved::get_saved_connections,
            commands::saved::save_connection,
            commands::saved::delete_saved_connection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
