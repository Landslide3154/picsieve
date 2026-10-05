use crate::db::Db;
use crate::error::{AppError, Result};
use crate::model::FileRecord;
use rayon::prelude::*;
use rusqlite::params;
use serde::Serialize;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HashStats {
    pub groups: u64,
    pub hashed: u64,
    pub failed: u64,
}

/// 打开文件，失败时最多重试 3 次、每次间隔 200 ms。
/// 用来对抗「文件正被别的程序占用」这类瞬时错误（规格第 12 节）。
pub fn open_with_retry(path: &Path) -> Result<std::fs::File> {
    let mut last: Option<std::io::Error> = None;
    for attempt in 0..4u32 {
        match std::fs::File::open(path) {
            Ok(f) => return Ok(f),
            Err(e) => {
                last = Some(e);
                if attempt < 3 {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
            }
        }
    }
    Err(AppError::io(path, last.expect("循环至少执行一次")))
}

/// 整个文件内容的 BLAKE3 十六进制摘要。采用 256 位，避免把不同文件误判为相同。
pub fn hash_file(path: &Path) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut f = open_with_retry(path)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| AppError::io(path, e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// 取出「文件大小重复」的分组，每组返回该组内的全部文件记录。
/// 大小唯一的文件直接排除，不必读取内容。
pub fn candidate_groups(db: &Db) -> Result<Vec<Vec<FileRecord>>> {
    let sizes: Vec<i64> = db.query_column(
        "SELECT size FROM files WHERE status='normal' GROUP BY size HAVING COUNT(*) > 1",
        params![],
    )?;
    let mut out = Vec::new();
    for size in sizes {
        let recs = db.query_files_by_size(size)?;
        if recs.len() > 1 {
            out.push(recs);
        }
    }
    Ok(out)
}

/// 第一遍指纹：只对候选组读取内容、算 BLAKE3、写回数据库。
pub fn fingerprint_content(
    db: &Db,
    threads: usize,
    progress: &mut dyn FnMut(HashStats),
) -> Result<HashStats> {
    let groups = candidate_groups(db)?;
    let mut stats = HashStats {
        groups: groups.len() as u64,
        ..Default::default()
    };

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .build()
        .map_err(|e| AppError::Other(format!("线程池创建失败: {e}")))?;

    for group in &groups {
        let results: Vec<(i64, std::result::Result<String, ()>)> = pool.install(|| {
            group
                .par_iter()
                .filter(|r| r.content_hash.is_none())
                .map(|r| {
                    let h = hash_file(Path::new(&r.path)).map_err(|_| ());
                    (r.id, h)
                })
                .collect()
        });
        for (id, h) in results {
            match h {
                Ok(hex) => {
                    db.set_content_hash(id, &hex)?;
                    stats.hashed += 1;
                }
                Err(_) => stats.failed += 1,
            }
        }
        progress(stats.clone());
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::model::FileRecord;

    fn rec(path: &str, size: i64) -> FileRecord {
        FileRecord {
            path: path.into(),
            root: "r".into(),
            size,
            mtime: 1,
            ..Default::default()
        }
    }

    #[test]
    fn open_with_retry_opens_existing_and_reports_missing() {
        let dir = tempfile::tempdir().unwrap();
        let ok = dir.path().join("ok.bin");
        std::fs::write(&ok, b"x").unwrap();
        assert!(open_with_retry(&ok).is_ok());
        assert!(open_with_retry(&dir.path().join("missing.bin")).is_err());
    }

    #[test]
    fn same_bytes_give_same_hash() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        std::fs::write(&a, b"hello picsieve").unwrap();
        std::fs::write(&b, b"hello picsieve").unwrap();
        assert_eq!(hash_file(&a).unwrap(), hash_file(&b).unwrap());
    }

    #[test]
    fn different_bytes_give_different_hash() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        std::fs::write(&a, b"hello picsieve").unwrap();
        std::fs::write(&b, b"hello picsieve!").unwrap();
        assert_ne!(hash_file(&a).unwrap(), hash_file(&b).unwrap());
    }

    #[test]
    fn only_size_collisions_become_candidates() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        // 三个 100 字节 + 一个独苗 200 字节
        for p in ["a", "b", "c"] {
            db.upsert_file(&rec(&format!("{p}.bin"), 100)).unwrap();
        }
        db.upsert_file(&rec("lonely.bin", 200)).unwrap();

        let groups = candidate_groups(&db).unwrap();
        assert_eq!(groups.len(), 1, "只有大小重复的那一组才该成为候选");
        assert_eq!(groups[0].len(), 3);
        assert!(!groups.iter().flatten().any(|r| r.path == "lonely.bin"));
    }

    #[test]
    fn fingerprint_fills_content_hash() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        std::fs::write(&a, vec![7u8; 4096]).unwrap();
        std::fs::write(&b, vec![7u8; 4096]).unwrap();

        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        db.upsert_file(&rec(&a.to_string_lossy(), 4096)).unwrap();
        db.upsert_file(&rec(&b.to_string_lossy(), 4096)).unwrap();

        let stats = fingerprint_content(&db, 2, &mut |_| {}).unwrap();
        assert_eq!(stats.hashed, 2);
        assert_eq!(stats.groups, 1);
    }
}
