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
        // 记住窗口位置、大小、是否最大化（存在 app 配置目录的 .window-state.json）。
        // 只留这三项：默认的「全部」里还包含可见性，万一在隐藏状态下退出，
        // 下次启动就会恢复成「窗口不可见」——那等于程序打不开，必须排除。
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
        .setup(|app| {
            // 用 LocalAppData：数据库与缩略图缓存体量大，不该跟着漫游配置文件走。
            //
            // 例外：设了环境变量 PICSIEVE_DATA_DIR 就用它。这是给端到端测试留的开关——
            // 「移入隔离区」这类会真动磁盘的流程必须在独立数据目录里跑，
            // 否则测试会污染用户自己的库（踩过：改 LOCALAPPDATA 对 Tauri 无效，
            // 它取的是系统「已知文件夹」API，结果测试数据写进了真实数据库）。
            let app_data = match std::env::var_os("PICSIEVE_DATA_DIR") {
                Some(dir) => std::path::PathBuf::from(dir),
                None => app.path().app_local_data_dir().expect("app data dir"),
            };
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

            // 窗口配置成 visible: false，等前端第一帧画好再显示，
            // 这样就不会出现「先在默认位置闪一下、再跳到上次的位置」。
            // 万一前端没起来（白屏），3 秒后也要把窗口露出来，别让人以为程序没启动。
            if let Some(w) = app.get_webview_window("main") {
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    if !w.is_visible().unwrap_or(true) {
                        let _ = w.show();
                    }
                });
            }
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
            commands::get_preview,
            commands::open_external,
            commands::reveal_in_explorer,
            commands::histogram_cmd,
            commands::format_stats_cmd,
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
            commands::show_main_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
