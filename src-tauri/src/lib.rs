pub mod commands;
pub mod db;
pub mod error;
pub mod model;
pub mod nameparse;
pub mod scanner;
pub mod settings;

use commands::AppState;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 用 LocalAppData：数据库与缩略图缓存体量大，不该跟着漫游配置文件走。
            let app_data = app.path().app_local_data_dir().expect("app data dir");
            std::fs::create_dir_all(&app_data).ok();
            let db = db::Db::open(&app_data.join("picsieve.db")).expect("open db");
            db.migrate().expect("migrate");
            let s = settings::load(&app_data);
            app.manage(AppState {
                db: Arc::new(db),
                app_data,
                settings: parking_lot::Mutex::new(s),
                cancel: Arc::new(AtomicBool::new(false)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::start_scan,
            commands::cancel_scan,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
