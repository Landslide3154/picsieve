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

/// 取消正在跑的扫描或指纹计算。两个任务共用同一个标记：
/// 同一时刻只会有一个在跑，取消谁都是取消当前这个。
#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>) {
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
        scanner::scan(
            &db,
            &roots,
            &ScanOptions { threads },
            &cancel,
            &mut on_progress,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn start_fingerprint(app: AppHandle) -> std::result::Result<serde_json::Value, String> {
    // 与 start_scan 同理：指纹计算是阻塞重活，必须丢进线程池，否则界面假死。
    let (threads, db, cancel) = {
        let state = app.state::<AppState>();
        let threads = state.settings.lock().threads;
        (threads, state.db.clone(), state.cancel.clone())
    };
    cancel.store(false, Ordering::SeqCst);

    tauri::async_runtime::spawn_blocking(move || {
        let mut cb = |s: crate::hashing::HashStats| {
            let _ = app.emit(
                "fingerprint://progress",
                serde_json::json!({ "phase": "content", "stats": s }),
            );
        };
        let content = crate::hashing::fingerprint_content(&db, threads, &cancel, &mut cb)
            .map_err(|e| e.to_string())?;

        let mut cb2 = |s: crate::fingerprint::VisualStats| {
            let _ = app.emit(
                "fingerprint://progress",
                serde_json::json!({ "phase": "visual", "stats": s }),
            );
        };
        let visual = crate::fingerprint::fingerprint_visual(&db, threads, &cancel, &mut cb2)
            .map_err(|e| e.to_string())?;

        Ok::<_, String>(serde_json::json!({ "content": content, "visual": visual }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 用系统默认程序打开图片（双击缩略图）。
///
/// 走 `cmd /C start "" "<路径>"`：路径来自数据库（由扫描得到），Windows 文件名本身不含引号，
/// 这里再用 raw_arg 手工加引号，避免文件名里的 `&` 被 cmd 当成命令分隔符。
#[tauri::command]
pub fn open_external(path: String) -> std::result::Result<(), String> {
    use std::os::windows::process::CommandExt;
    let safe = path.replace('"', "");
    std::process::Command::new("cmd")
        .arg("/C")
        .raw_arg(format!("start \"\" \"{safe}\""))
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打不开这个文件：{e}"))
}

/// explorer 的参数形状必须是 `/select,"<路径>"`：开关在外面、路径在里面。
///
/// 这里不能交给 Rust 自动加引号——路径含空格时它会生成 `"/select,C:\a b.jpg"`，
/// 整个开关被包进引号，explorer 就当普通路径处理，结果打开的是默认位置（用户反馈的 bug）。
fn reveal_arg(path: &str) -> String {
    format!("/select,\"{}\"", path.replace('"', ""))
}

/// 在资源管理器里定位到这个文件。
#[tauri::command]
pub fn reveal_in_explorer(path: String) -> std::result::Result<(), String> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("explorer")
        .raw_arg(reveal_arg(&path))
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打不开资源管理器：{e}"))
}

/// 筛选栏的分布直方图。kind = "size" | "pixels"。
#[tauri::command]
pub async fn histogram_cmd(
    app: AppHandle,
    kind: String,
) -> std::result::Result<crate::model::Histogram, String> {
    let db = app.state::<AppState>().db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let r = match kind.as_str() {
            "size" => db.histogram_size(),
            "pixels" => db.histogram_pixels(),
            _ => db.histogram_short_side(),
        };
        r.map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 库里实际存在的格式与张数（格式选项不写死，按数据来）。
#[tauri::command]
pub fn format_stats_cmd(
    state: State<'_, AppState>,
) -> std::result::Result<Vec<crate::model::FormatStat>, String> {
    state.db.format_stats().map_err(|e| e.to_string())
}

/// 顶栏用的库内总体情况。
#[tauri::command]
pub fn library_stats(
    state: State<'_, AppState>,
) -> std::result::Result<crate::model::LibraryStats, String> {
    state.db.library_stats().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn count_dup_groups(
    state: State<'_, AppState>,
    kind: String,
) -> std::result::Result<i64, String> {
    state.db.count_groups(&kind).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn quarantine_batch_files(
    state: State<'_, AppState>,
    batch: String,
) -> std::result::Result<Vec<FileRecord>, String> {
    state
        .db
        .quarantine_batch_files(&batch)
        .map_err(|e| e.to_string())
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

/// 取缩略图。
///
/// **必须离开界面主线程**：同步命令会在 WebView 的主线程上执行，一张大图解码要几百毫秒，
/// 一屏几十张就是十几秒的界面假死（用户反馈的「拖滑块卡顿」很大一部分来自这里）。
/// 并发限制在前端排队（见 src/api.ts），后端不再自建信号量。
#[tauri::command]
pub async fn get_thumb(
    app: AppHandle,
    file_id: i64,
) -> std::result::Result<tauri::ipc::Response, String> {
    let (rec, max_edge, cache) = {
        let state = app.state::<AppState>();
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
        (rec, max_edge, cache)
    };

    tauri::async_runtime::spawn_blocking(move || {
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
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 空格预览用的大图：长边 1600，单独一份缓存目录（原图不复制、不改动）。
#[tauri::command]
pub async fn get_preview(
    app: AppHandle,
    file_id: i64,
) -> std::result::Result<tauri::ipc::Response, String> {
    let (rec, cache) = {
        let state = app.state::<AppState>();
        let rec = state
            .db
            .get_file(file_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "文件不存在".to_string())?;
        let cache = app
            .path()
            .app_cache_dir()
            .map_err(|e| e.to_string())?
            .join("previews");
        (rec, cache)
    };

    tauri::async_runtime::spawn_blocking(move || {
        let p = crate::thumb::make_thumb(
            std::path::Path::new(&rec.path),
            &cache,
            rec.id,
            rec.mtime,
            1600,
        )
        .map_err(|e| e.to_string())?;
        let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
        Ok(tauri::ipc::Response::new(bytes))
    })
    .await
    .map_err(|e| e.to_string())?
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

/// 移入隔离区。跨盘时会退化成「复制 + 校验指纹 + 删源」，可能很慢，必须离开界面线程。
#[tauri::command]
pub async fn move_to_quarantine(
    app: AppHandle,
    file_ids: Vec<i64>,
) -> std::result::Result<crate::quarantine::MoveReport, String> {
    let (dir, db) = {
        let state = app.state::<AppState>();
        let dir = state.settings.lock().quarantine_dir.clone();
        (dir, state.db.clone())
    };
    let batch = uuid::Uuid::new_v4().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        crate::quarantine::move_in(&db, &file_ids, std::path::Path::new(&dir), &batch)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn restore_batch(
    app: AppHandle,
    batch: String,
) -> std::result::Result<crate::quarantine::MoveReport, String> {
    let db = app.state::<AppState>().db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::quarantine::restore(&db, &batch).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn purge_batch(
    app: AppHandle,
    batch: String,
) -> std::result::Result<crate::quarantine::PurgeReport, String> {
    let db = app.state::<AppState>().db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::quarantine::purge(&db, &batch).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn list_quarantine_batches(
    state: State<'_, AppState>,
) -> std::result::Result<Vec<crate::model::QuarantineBatch>, String> {
    state.db.quarantine_batches().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn thumb_cache_stats(app: AppHandle) -> std::result::Result<u64, String> {
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("thumbs");
    tauri::async_runtime::spawn_blocking(move || crate::thumb::cache_size_bytes(&cache))
        .await
        .map_err(|e| e.to_string())
}

/// 按设置里的上限回收缩略图缓存。缓存目录可能有几万个文件，必须离开界面线程。
#[tauri::command]
pub async fn trim_thumb_cache(
    app: AppHandle,
) -> std::result::Result<crate::thumb::TrimReport, String> {
    let (cache, limit_mb) = {
        let state = app.state::<AppState>();
        let limit = state.settings.lock().thumb_cache_limit_mb;
        let cache = app
            .path()
            .app_cache_dir()
            .map_err(|e| e.to_string())?
            .join("thumbs");
        (cache, limit)
    };
    tauri::async_runtime::spawn_blocking(move || {
        crate::thumb::trim_cache(&cache, limit_mb.saturating_mul(1024 * 1024))
    })
    .await
    .map_err(|e| e.to_string())
}

/// 前端第一帧画好后调用：把窗口显示出来。
///
/// 窗口在配置里是 `visible: false`，为的是等 `window-state` 插件恢复完位置/大小
/// 再露脸，避免「先在默认位置闪一下、再跳到上次的位置」。
#[tauri::command]
pub fn show_main_window(app: AppHandle) -> std::result::Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        w.show().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::reveal_arg;

    /// explorer 的开关必须留在引号外面，否则它会当成普通路径、打开默认位置。
    #[test]
    fn reveal_argument_keeps_switch_outside_quotes() {
        assert_eq!(reveal_arg(r"D:\色图\a.jpg"), "/select,\"D:\\色图\\a.jpg\"");
        assert_eq!(
            reveal_arg(r"D:\色图\2017-2024 PIXIV daily\a b.jpg"),
            "/select,\"D:\\色图\\2017-2024 PIXIV daily\\a b.jpg\"",
            "含空格的路径必须只把路径括起来"
        );
        assert_eq!(
            reveal_arg("D:\\x\"y.jpg"),
            "/select,\"D:\\xy.jpg\"",
            "路径里的引号要剔除"
        );
    }
}
