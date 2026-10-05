use crate::db::{now_secs, Db};
use crate::error::{AppError, Result};
use crate::model::{FileRecord, ScanStats};
use crate::nameparse;
use rayon::prelude::*;
use std::io::Read;
use std::path::{Path, PathBuf};

/// 只处理这些扩展名的文件；其余只计数不入库。
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];

/// 每攒够这么多条记录写一次库（一个事务）。
const WRITE_BATCH: usize = 2048;

/// 读图片头时先一次性读进这么多字节再解析；见 `read_header` 的说明。
const HEADER_PREFIX: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub threads: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        let n = std::thread::available_parallelism()
            .map(|v| v.get())
            .unwrap_or(4);
        Self {
            threads: n.saturating_sub(2).max(1),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub seen: u64,
    pub total_hint: u64,
    pub current: String,
}

pub fn is_image_ext(ext: &str) -> bool {
    IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

/// 遍历 roots，把图片文件的元数据写入数据库。
///
/// 已入库且 `size` 与 `mtime` 都没变的对象直接跳过（增量）。
/// 单个文件出错只记录、不中断整体。
pub fn scan(
    db: &Db,
    roots: &[PathBuf],
    opts: &ScanOptions,
    progress: &mut dyn FnMut(ScanProgress),
) -> Result<ScanStats> {
    for r in roots {
        if !r.is_dir() {
            return Err(AppError::NotADirectory(r.clone()));
        }
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(opts.threads)
        .build()
        .map_err(|e| AppError::Other(format!("线程池创建失败: {e}")))?;

    // 收集待处理文件，同时记住它属于哪个扫描根目录
    let mut candidates: Vec<(PathBuf, PathBuf)> = Vec::new();
    for root in roots {
        for entry in jwalk::WalkDir::new(root).skip_hidden(false) {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if is_image_ext(ext) {
                candidates.push((path, root.clone()));
            }
        }
    }

    let total_hint = candidates.len() as u64;
    let known = db.path_fingerprints()?;

    let results: Vec<std::result::Result<FileRecord, ()>> = pool.install(|| {
        candidates
            .par_iter()
            .map(|(path, root)| build_record(path, root).map_err(|_| ()))
            .collect()
    });

    let mut stats = ScanStats::default();
    let mut seen = 0u64;
    // 攒批写库：逐条提交会让十几万次 INSERT 各自提交一次，实测能吃掉一半扫描时间
    let mut pending: Vec<FileRecord> = Vec::with_capacity(WRITE_BATCH);
    for ((path, _root), res) in candidates.iter().zip(results) {
        seen += 1;
        match res {
            Ok(rec) => {
                let key = path.to_string_lossy().to_string();
                let unchanged = known
                    .get(&key)
                    .map(|(_, size, mtime)| *size == rec.size && *mtime == rec.mtime)
                    .unwrap_or(false);
                if unchanged {
                    stats.skipped += 1;
                } else {
                    let existed = known.contains_key(&key);
                    pending.push(rec);
                    if existed {
                        stats.updated += 1;
                    } else {
                        stats.inserted += 1;
                    }
                    if pending.len() >= WRITE_BATCH {
                        db.upsert_files(&pending)?;
                        pending.clear();
                    }
                }
            }
            // 只统计「连元数据都读不到」的文件；图片头坏了的不算这里，见 build_record
            Err(_) => stats.failed += 1,
        }
        if seen.is_multiple_of(500) || seen == total_hint {
            progress(ScanProgress {
                seen,
                total_hint,
                current: path.to_string_lossy().to_string(),
            });
        }
    }
    if !pending.is_empty() {
        db.upsert_files(&pending)?;
    }
    stats.seen = seen;
    Ok(stats)
}

fn build_record(path: &Path, root: &Path) -> Result<FileRecord> {
    let meta = std::fs::metadata(path).map_err(|e| AppError::io(path, e))?;
    let size = meta.len() as i64;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    // 只读图片头拿宽高与真实格式。
    // 头坏掉的文件**照样入库**并记下 decode_error：规格第 3.2 节要求「是否解码失败」
    // 是一个可筛的维度，第 12 节要求记录后继续。若在这里直接丢弃，界面上就永远查不到它们。
    let (format, width, height, decode_error) = match read_header(path) {
        Ok((format, w, h)) => (format, Some(w as i64), Some(h as i64), None),
        Err(e) => (None, None, None, Some(e.to_string())),
    };

    Ok(FileRecord {
        id: 0,
        path: path.to_string_lossy().to_string(),
        root: root.to_string_lossy().to_string(),
        size,
        mtime,
        ext,
        format,
        width,
        height,
        short_side: width.zip(height).map(|(w, h)| w.min(h)),
        pid: nameparse::parse_pid(stem),
        artist: nameparse::parse_artist(stem),
        content_hash: None,
        phash: None,
        gray_score: None,
        decode_error,
        scanned_at: Some(now_secs()),
        fingerprinted_at: None,
        status: "normal".into(),
    })
}

/// 只读图片头，返回真实格式与宽高。
///
/// 性能上有个坑：直接让 `image` 打开文件走解码器时，它会做大量零散的小读取，
/// 实测（本机 2TB QLC SSD，5000 个真实文件）比「一次性读进 64KB 再在内存里解析」
/// 慢 13～67 倍，且并行度上不去。所以默认走前缀读法，
/// 少数把 SOF 放在 64KB 之后的 JPEG（超大 EXIF）解析不出来时，再退回完整读取。
fn read_header(path: &Path) -> Result<(Option<String>, u32, u32)> {
    if let Some(v) = read_header_from_prefix(path) {
        return Ok(v);
    }
    read_header_full(path)
}

fn read_header_from_prefix(path: &Path) -> Option<(Option<String>, u32, u32)> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; HEADER_PREFIX];
    let mut n = 0usize;
    while n < buf.len() {
        match f.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(_) => return None,
        }
    }
    header_from(std::io::Cursor::new(&buf[..n]))
}

fn read_header_full(path: &Path) -> Result<(Option<String>, u32, u32)> {
    let reader = image::ImageReader::open(path)
        .map_err(|e| AppError::io(path, e))?
        .with_guessed_format()
        .map_err(|e| AppError::io(path, e))?;
    let format = reader
        .format()
        .map(|f| format!("{f:?}").to_ascii_lowercase());
    let (w, h) = reader.into_dimensions().map_err(|e| AppError::Image {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok((format, w, h))
}

fn header_from<R: std::io::BufRead + std::io::Seek>(src: R) -> Option<(Option<String>, u32, u32)> {
    let reader = image::ImageReader::new(src).with_guessed_format().ok()?;
    let format = reader
        .format()
        .map(|f| format!("{f:?}").to_ascii_lowercase());
    let (w, h) = reader.into_dimensions().ok()?;
    Some((format, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use image::{Rgb, RgbImage};
    use std::fs;

    fn make_png(path: &std::path::Path, w: u32, h: u32) {
        let mut img = RgbImage::new(w, h);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = Rgb([(x % 256) as u8, (y % 256) as u8, 128]);
        }
        img.save(path).expect("save png");
    }

    #[test]
    fn scans_images_and_records_dimensions() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        make_png(&root.join("a.png"), 800, 600);
        make_png(&root.join("b.png"), 400, 1000);
        fs::write(root.join("note.txt"), b"not an image").expect("write");

        let db = Db::open_in_memory().expect("db");
        db.migrate().expect("migrate");
        let stats = scan(
            &db,
            &[root.to_path_buf()],
            &ScanOptions::default(),
            &mut |_| {},
        )
        .expect("scan");

        assert_eq!(stats.inserted, 2, "两次图片应入库，txt 应被忽略");
        assert_eq!(stats.failed, 0);
        let a = db
            .find_by_path(&root.join("a.png").to_string_lossy())
            .expect("q")
            .expect("some");
        assert_eq!(
            (a.width, a.height, a.short_side),
            (Some(800), Some(600), Some(600))
        );
        assert_eq!(a.format.as_deref(), Some("png"));
    }

    #[test]
    fn second_scan_skips_unchanged_files() {
        let dir = tempfile::tempdir().expect("tmp");
        make_png(&dir.path().join("a.png"), 100, 100);
        let db = Db::open_in_memory().expect("db");
        db.migrate().expect("migrate");

        scan(
            &db,
            &[dir.path().to_path_buf()],
            &ScanOptions::default(),
            &mut |_| {},
        )
        .expect("first");
        let second = scan(
            &db,
            &[dir.path().to_path_buf()],
            &ScanOptions::default(),
            &mut |_| {},
        )
        .expect("second");

        assert_eq!(second.inserted, 0);
        assert_eq!(second.skipped, 1, "大小与修改时间都没变，应跳过");
    }

    #[test]
    fn corrupt_image_is_recorded_as_decode_error() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(dir.path().join("broken.jpg"), b"\xFF\xD8\xFF\xE0garbage").expect("write");
        let db = Db::open_in_memory().expect("db");
        db.migrate().expect("migrate");
        let stats = scan(
            &db,
            &[dir.path().to_path_buf()],
            &ScanOptions::default(),
            &mut |_| {},
        )
        .expect("scan");

        assert_eq!(
            stats.inserted, 1,
            "头读不出的文件也要入库，否则「只看读不出的」永远是空的"
        );
        assert_eq!(stats.failed, 0, "failed 只统计连元数据都读不到的");
        let rec = db
            .find_by_path(&dir.path().join("broken.jpg").to_string_lossy())
            .expect("q")
            .expect("some");
        assert!(rec.decode_error.is_some(), "必须记下解码失败原因");
        assert_eq!((rec.width, rec.height), (None, None));
    }

    #[test]
    fn unreadable_path_counts_as_failed() {
        let dir = tempfile::tempdir().expect("tmp");
        let missing = dir.path().join("gone.png");
        std::fs::write(&missing, b"x").expect("write");
        std::fs::remove_file(&missing).expect("remove");
        // 目录本身仍存在，因此校验通过；但候选清单里塞不进不存在的文件，
        // 这里直接验证 build_record 对缺失文件返回 Err
        assert!(build_record(&missing, dir.path()).is_err());
    }

    /// 前缀读取是扫描的主要提速手段，必须与完整读取给出同样的宽高与格式。
    #[test]
    fn prefix_header_matches_full_read() {
        let dir = tempfile::tempdir().expect("tmp");
        let png = dir.path().join("a.png");
        let jpg = dir.path().join("b.jpg");
        make_png(&png, 321, 654);
        let mut img = RgbImage::new(777, 123);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = Rgb([(x % 256) as u8, (y % 256) as u8, 7]);
        }
        img.save(&jpg).expect("save jpg");

        for p in [&png, &jpg] {
            let fast = read_header_from_prefix(p).expect("前缀读取应成功");
            let full = read_header_full(p).expect("完整读取应成功");
            assert_eq!(fast, full, "两种读法必须一致: {}", p.display());
        }
        assert_eq!(
            read_header_from_prefix(&png).unwrap(),
            (Some("png".to_string()), 321, 654)
        );
        assert_eq!(
            read_header_from_prefix(&jpg).unwrap(),
            (Some("jpeg".to_string()), 777, 123)
        );
    }
}
