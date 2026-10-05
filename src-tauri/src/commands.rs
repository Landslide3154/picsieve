use crate::db::Db;
use crate::model::{FileRecord, Filter, ScanStats};
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

#[tauri::command]
pub async fn start_fingerprint(app: AppHandle) -> std::result::Result<serde_json::Value, String> {
    // 与 start_scan 同理：指纹计算是阻塞重活，必须丢进线程池，否则界面假死。
    let (threads, db) = {
        let state = app.state::<AppState>();
        let threads = state.settings.lock().threads;
        (threads, state.db.clone())
    };

    tauri::async_runtime::spawn_blocking(move || {
        let mut cb = |s: crate::hashing::HashStats| {
            let _ = app.emit(
                "fingerprint://progress",
                serde_json::json!({ "phase": "content", "stats": s }),
            );
        };
        let content = crate::hashing::fingerprint_content(&db, threads, &mut cb)
            .map_err(|e| e.to_string())?;

        let mut cb2 = |s: crate::fingerprint::VisualStats| {
            let _ = app.emit(
                "fingerprint://progress",
                serde_json::json!({ "phase": "visual", "stats": s }),
            );
        };
        let visual = crate::fingerprint::fingerprint_visual(&db, threads, &mut cb2)
            .map_err(|e| e.to_string())?;

        Ok::<_, String>(serde_json::json!({ "content": content, "visual": visual }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn query_files_cmd(
    state: State<'_, AppState>,
    filter: Filter,
) -> std::result::Result<Vec<FileRecord>, String> {
    let gray = state.settings.lock().gray_threshold;
    crate::query::query_files_gray(&state.db, &filter, gray).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn count_files_cmd(
    state: State<'_, AppState>,
    filter: Filter,
) -> std::result::Result<i64, String> {
    let gray = state.settings.lock().gray_threshold;
    crate::query::count_files_gray(&state.db, &filter, gray).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_thumb(
    app: AppHandle,
    state: State<'_, AppState>,
    file_id: i64,
) -> std::result::Result<tauri::ipc::Response, String> {
    let rec = state
        .db
        .get_file(file_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "文件不存在".to_string())?;
    let max_edge = state.settings.lock().thumb_max_edge;
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("thumbs");
    let p = crate::thumb::make_thumb(
        std::path::Path::new(&rec.path),
        &cache,
        rec.id,
        rec.mtime,
        max_edge,
    )
    .map_err(|e| e.to_string())?;
    // 用 Response 直接回二进制：几千张缩略图若走 JSON 数组会白烧 CPU 和内存
    let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub fn list_dup_groups(
    state: State<'_, AppState>,
    kind: String,
    offset: i64,
    limit: i64,
) -> std::result::Result<Vec<crate::model::GroupView>, String> {
    state
        .db
        .list_groups(&kind, offset, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_keeper(
    state: State<'_, AppState>,
    group_id: i64,
    file_id: i64,
) -> std::result::Result<(), String> {
    state
        .db
        .set_group_keeper(group_id, file_id)
        .map_err(|e| e.to_string())
}

/// 重建分组。相似聚类是 O(n²) 的纯计算活，必须放到阻塞线程池，否则界面会假死。
#[tauri::command]
pub async fn rebuild_groups(app: AppHandle) -> std::result::Result<serde_json::Value, String> {
    let (threshold, threads, db) = {
        let state = app.state::<AppState>();
        let (threshold, threads) = {
            let s = state.settings.lock();
            (s.similar_threshold, s.threads)
        };
        (threshold, threads, state.db.clone())
    };

    tauri::async_runtime::spawn_blocking(move || {
        let exact = crate::grouper::group_exact(&db).map_err(|e| e.to_string())?;
        crate::grouper::persist_exact(&db, &exact).map_err(|e| e.to_string())?;
        let similar =
            crate::grouper::group_similar(&db, threshold, threads).map_err(|e| e.to_string())?;
        crate::grouper::persist_similar(&db, &similar, threshold).map_err(|e| e.to_string())?;
        Ok::<_, String>(serde_json::json!({ "exact": exact.len(), "similar": similar.len() }))
    })
    .await
    .map_err(|e| e.to_string())?
}
