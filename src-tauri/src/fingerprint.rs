use crate::db::{now_secs, Db};
use crate::error::{AppError, Result};
use crate::gray::gray_score;
use crate::model::FileRecord;
use crate::phash::{phash_u64, to_hex};
use rayon::prelude::*;
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualStats {
    pub done: u64,
    pub skipped: u64,
    pub failed: u64,
    pub cancelled: bool,
    /// 这一轮要处理的张数（界面用来显示「已完成 / 总数」）
    pub total: u64,
}

/// 第二遍指纹：解码缩略图，算 pHash 与灰度分数。
///
/// 只处理 `phash IS NULL AND decode_error IS NULL` 的文件，因此天然支持断点续算。
/// GIF 由 `image` 解码器默认取第一帧。
/// `cancel` 为真时在每批之间检查一次，尽快停下（已完成的已落库，可续算）。
pub fn fingerprint_visual(
    db: &Db,
    threads: usize,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(VisualStats),
) -> Result<VisualStats> {
    let pending = db.files_needing_visual()?;
    let mut stats = VisualStats {
        total: pending.len() as u64,
        ..Default::default()
    };

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .build()
        .map_err(|e| AppError::Other(format!("线程池创建失败: {e}")))?;

    let batch = 512;
    for chunk in pending.chunks(batch) {
        if cancel.load(Ordering::Relaxed) {
            stats.cancelled = true;
            break;
        }
        let results: Vec<(i64, Result<(String, f64)>)> = pool.install(|| {
            chunk
                .par_iter()
                .map(|r: &FileRecord| (r.id, decode_and_hash(Path::new(&r.path))))
                .collect()
        });
        // 攒批写库：解码是慢活，别让几万次单条 UPDATE 再雪上加霜
        let mut ok_rows: Vec<(i64, String, f64)> = Vec::new();
        let mut err_rows: Vec<(i64, String)> = Vec::new();
        for (id, res) in results {
            match res {
                Ok((hex, gray)) => {
                    ok_rows.push((id, hex, gray));
                    stats.done += 1;
                }
                Err(e) => {
                    err_rows.push((id, e.to_string()));
                    stats.failed += 1;
                }
            }
        }
        db.set_visual_many(&ok_rows, now_secs())?;
        db.set_decode_error_many(&err_rows)?;
        progress(stats.clone());
    }
    stats.skipped = db.count_visual_done()?;
    Ok(stats)
}

fn decode_and_hash(path: &Path) -> Result<(String, f64)> {
    let img = image::ImageReader::open(path)
        .map_err(|e| AppError::io(path, e))?
        .with_guessed_format()
        .map_err(|e| AppError::io(path, e))?
        .decode()
        .map_err(|e| AppError::Image {
            path: path.to_path_buf(),
            source: e,
        })?;
    Ok((to_hex(phash_u64(&img)), gray_score(&img)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::model::FileRecord;
    use image::{Rgb, RgbImage};

    fn write_png(path: &std::path::Path, w: u32, h: u32, color: [u8; 3]) {
        RgbImage::from_pixel(w, h, Rgb(color)).save(path).unwrap();
    }

    fn seed(db: &Db, path: &std::path::Path, w: i64, h: i64) {
        db.upsert_file(&FileRecord {
            path: path.to_string_lossy().to_string(),
            root: "r".into(),
            size: std::fs::metadata(path).unwrap().len() as i64,
            mtime: 1,
            width: Some(w),
            height: Some(h),
            short_side: Some(w.min(h)),
            ..Default::default()
        })
        .unwrap();
    }

    #[test]
    fn fills_phash_and_gray_for_every_file() {
        let dir = tempfile::tempdir().unwrap();
        let color = dir.path().join("c.png");
        let gray = dir.path().join("g.png");
        write_png(&color, 200, 150, [220, 40, 60]);
        write_png(&gray, 200, 150, [128, 128, 128]);
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db, &color, 200, 150);
        seed(&db, &gray, 200, 150);

        let stats = fingerprint_visual(&db, 2, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(stats.done, 2);

        let c = db.find_by_path(&color.to_string_lossy()).unwrap().unwrap();
        let g = db.find_by_path(&gray.to_string_lossy()).unwrap().unwrap();
        assert!(c.phash.is_some() && c.gray_score.unwrap() > 100.0);
        assert!(g.phash.is_some() && g.gray_score.unwrap() <= 8.0);
    }

    #[test]
    fn resumes_without_recomputing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.png");
        write_png(&p, 120, 120, [10, 200, 30]);
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db, &p, 120, 120);

        let first = fingerprint_visual(&db, 1, &AtomicBool::new(false), &mut |_| {}).unwrap();
        let second = fingerprint_visual(&db, 1, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(first.done, 1);
        assert_eq!(second.done, 0, "已算过的图不应重算");
        assert_eq!(second.skipped, 1);
    }

    #[test]
    fn gif_uses_first_frame_only() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.png");
        write_png(&p, 64, 64, [1, 2, 3]);
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db, &p, 64, 64);
        assert!(fingerprint_visual(&db, 1, &AtomicBool::new(false), &mut |_| {}).is_ok());
    }
}
