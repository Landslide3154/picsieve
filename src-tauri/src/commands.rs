use crate::db::Db;
use crate::model::ScanStats;
use crate::scanner::{self, ScanOptions};
use crate::settings::{self, Settings};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    /// 用 Arc 包住，才能在线程池里做扫描/指纹而不阻塞界面线程。
    pub db: Arc<Db>,
    pub app_data: PathBuf,
    pub settings: parking_lot::Mutex<Settings>,
    pub cancel: Arc<AtomicBool>,
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().clone()
}

#[tauri::command]
pub fn save_settings(
    state: State<'_, AppState>,
    settings: Settings,
) -> std::result::Result<(), String> {
    settings::validate_quarantine_dir(&settings.quarantine_dir, &settings.roots)
        .map_err(|e| e.to_string())?;
    settings::save(&state.app_data, &settings).map_err(|e| e.to_string())?;
    *state.settings.lock() = settings;
    Ok(())
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn start_scan(app: AppHandle) -> std::result::Result<ScanStats, String> {
    // 先把要用的东西取出来并克隆成拥有所有权的值，再丢进线程池。
    // 扫描是重 IO + CPU 的活，绝不能占着 async 运行时线程，否则界面会假死。
    let (roots, threads, db, cancel) = {
        let state = app.state::<AppState>();
        let settings = state.settings.lock();
        let roots: Vec<PathBuf> = settings.roots.iter().map(PathBuf::from).collect();
        (
            roots,
            settings.threads,
            state.db.clone(),
            state.cancel.clone(),
        )
    };
    if roots.is_empty() {
        return Err("还没有添加要扫描的文件夹".into());
    }
    cancel.store(false, Ordering::SeqCst);

    tauri::async_runtime::spawn_blocking(move || {
        let mut on_progress = |p: scanner::ScanProgress| {
            let _ = app.emit("scan://progress", p);
        };
        scanner::scan(&db, &roots, &ScanOptions { threads }, &mut on_progress)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
