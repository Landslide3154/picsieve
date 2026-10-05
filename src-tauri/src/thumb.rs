use crate::error::{AppError, Result};
use image::imageops::FilterType;
use std::path::{Path, PathBuf};

/// 缓存文件名带上 mtime：源文件被替换后自动失效，不需要额外的清理逻辑。
pub fn thumb_path(cache_dir: &Path, id: i64, mtime: i64) -> PathBuf {
    cache_dir.join(format!("{id}_{mtime}.jpg"))
}

/// 生成（或复用）缩略图，返回缓存文件路径。
pub fn make_thumb(
    src: &Path,
    cache_dir: &Path,
    id: i64,
    mtime: i64,
    max_edge: u32,
) -> Result<PathBuf> {
    let out = thumb_path(cache_dir, id, mtime);
    if out.is_file() {
        return Ok(out);
    }
    std::fs::create_dir_all(cache_dir).map_err(|e| AppError::io(cache_dir, e))?;

    let img = image::ImageReader::open(src)
        .map_err(|e| AppError::io(src, e))?
        .with_guessed_format()
        .map_err(|e| AppError::io(src, e))?
        .decode()
        .map_err(|e| AppError::Image {
            path: src.to_path_buf(),
            source: e,
        })?;

    let thumb = img.resize(max_edge, max_edge, FilterType::Triangle);
    // 先写临时文件再改名，避免进程中途退出留下半截 JPEG。
    // 临时名带唯一后缀：界面滚动时同一张图可能被并发请求两次，不能互相踩。
    let tmp = out.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    thumb
        .to_rgb8()
        .save_with_format(&tmp, image::ImageFormat::Jpeg)
        .map_err(|e| AppError::Image {
            path: tmp.clone(),
            source: e,
        })?;
    std::fs::rename(&tmp, &out).map_err(|e| AppError::io(&out, e))?;
    Ok(out)
}

/// 缓存目录当前占用字节数，供设置界面显示。
pub fn cache_size_bytes(cache_dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = std::fs::read_dir(cache_dir) {
        for e in rd.flatten() {
            if let Ok(m) = e.metadata() {
                total += m.len();
            }
        }
    }
    total
}

#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrimReport {
    pub removed: u64,
    pub freed: u64,
    pub remaining: u64,
}

/// 把缓存压到上限以内：按文件修改时间从旧到新删（旧的缩略图被重新浏览时再算一次就行）。
///
/// 一次删到上限的 80%，留出余量，避免每打开一次软件就做一轮删除。
/// 上限设为 0 视为不限。
pub fn trim_cache(cache_dir: &Path, limit_bytes: u64) -> TrimReport {
    let mut files: Vec<(std::time::SystemTime, PathBuf, u64)> = Vec::new();
    let mut total = 0u64;
    if let Ok(rd) = std::fs::read_dir(cache_dir) {
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if !m.is_file() {
                continue;
            }
            let t = m.modified().unwrap_or(std::time::UNIX_EPOCH);
            total += m.len();
            files.push((t, e.path(), m.len()));
        }
    }

    let mut report = TrimReport {
        remaining: total,
        ..Default::default()
    };
    if limit_bytes == 0 || total <= limit_bytes {
        return report;
    }

    let target = limit_bytes / 10 * 8;
    files.sort_by_key(|(t, _, _)| *t);
    for (_, path, size) in files {
        if report.remaining <= target {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            report.removed += 1;
            report.freed += size;
            report.remaining -= size;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn make(path: &std::path::Path, w: u32, h: u32) {
        RgbImage::from_pixel(w, h, Rgb([10, 200, 30]))
            .save(path)
            .unwrap();
    }

    #[test]
    fn thumb_is_smaller_than_source() {
        let src_dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let p = src_dir.path().join("a.png");
        make(&p, 1200, 900);

        let out = make_thumb(&p, cache.path(), 1, 100, 320).unwrap();
        let img = image::open(&out).unwrap();
        assert!(img.width().max(img.height()) <= 320);
    }

    #[test]
    fn second_call_reuses_cache() {
        let src_dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let p = src_dir.path().join("a.png");
        make(&p, 600, 600);

        let a = make_thumb(&p, cache.path(), 7, 100, 320).unwrap();
        let first_mtime = std::fs::metadata(&a).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        let b = make_thumb(&p, cache.path(), 7, 100, 320).unwrap();

        assert_eq!(a, b);
        assert_eq!(
            first_mtime,
            std::fs::metadata(&b).unwrap().modified().unwrap(),
            "应命中缓存，不重写"
        );
    }

    #[test]
    fn cache_dir_is_created_on_demand() {
        let src_dir = tempfile::tempdir().unwrap();
        let cache_root = tempfile::tempdir().unwrap();
        let nested = cache_root.path().join("deep").join("thumbs");
        let p = src_dir.path().join("a.png");
        make(&p, 200, 200);
        assert!(make_thumb(&p, &nested, 3, 55, 320).is_ok());
        assert!(nested.exists());
    }

    #[test]
    fn trim_cache_brings_total_under_limit() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..4 {
            std::fs::write(dir.path().join(format!("{i}_1.jpg")), vec![7u8; 1000]).unwrap();
            // 让 mtime 单调，删除顺序才确定
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        let before = cache_size_bytes(dir.path());
        assert_eq!(before, 4000);

        let r = trim_cache(dir.path(), 2500);
        assert!(r.removed >= 1, "超上限就该删");
        assert!(cache_size_bytes(dir.path()) <= 2500, "必须回到上限以内");
        assert_eq!(r.freed, before - cache_size_bytes(dir.path()));
        assert_eq!(r.remaining, cache_size_bytes(dir.path()));
    }

    #[test]
    fn trim_cache_is_noop_when_under_limit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("1_1.jpg"), vec![7u8; 1000]).unwrap();
        let r = trim_cache(dir.path(), 10_000);
        assert_eq!(r.removed, 0);
        assert_eq!(r.remaining, 1000);
        assert_eq!(cache_size_bytes(dir.path()), 1000);
    }

    #[test]
    fn trim_cache_treats_zero_limit_as_unlimited() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("1_1.jpg"), vec![7u8; 1000]).unwrap();
        let r = trim_cache(dir.path(), 0);
        assert_eq!(r.removed, 0);
        assert_eq!(r.remaining, 1000);
    }
}
