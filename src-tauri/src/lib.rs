pub mod commands;
pub mod db;
pub mod error;
pub mod fingerprint;
pub mod gray;
pub mod grouper;
pub mod hashing;
pub mod model;
pub mod nameparse;
pub mod phash;
pub mod quarantine;
pub mod query;
pub mod scanner;
pub mod settings;
pub mod thumb;

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
            // 后台按上限回收缩略图缓存：缓存目录可能有几万个文件，不能卡启动
            let cache_limit_mb = s.thumb_cache_limit_mb;
            if let Ok(cache_root) = app.path().app_cache_dir() {
                let cache = cache_root.join("thumbs");
                tauri::async_runtime::spawn_blocking(move || {
                    crate::thumb::trim_cache(&cache, cache_limit_mb.saturating_mul(1024 * 1024));
                });
            }
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
            commands::cancel_job,
            commands::start_fingerprint,
            commands::query_files_cmd,
            commands::count_files_cmd,
            commands::get_thumb,
            commands::open_external,
            commands::reveal_in_explorer,
            commands::histogram_cmd,
            commands::library_stats,
            commands::count_dup_groups,
            commands::quarantine_batch_files,
            commands::list_dup_groups,
            commands::set_keeper,
            commands::rebuild_groups,
            commands::move_to_quarantine,
            commands::restore_batch,
            commands::purge_batch,
            commands::list_quarantine_batches,
            commands::thumb_cache_stats,
            commands::trim_thumb_cache,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
