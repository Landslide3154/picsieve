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
}
