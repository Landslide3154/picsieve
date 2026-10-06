use crate::db::Db;
use crate::error::{AppError, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveReport {
    pub moved: u64,
    pub failed: u64,
    pub bytes: u64,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeReport {
    pub purged: u64,
    pub bytes: u64,
}

/// 把文件搬进隔离区。
///
/// 同一块盘上用 `rename`（原子、瞬时、不占额外空间）；跨盘时退化为
/// 「复制 → 校验 BLAKE3 → 删除源文件」，校验不过就中止并保留原文件。
pub fn move_in(db: &Db, ids: &[i64], quarantine_root: &Path, batch: &str) -> Result<MoveReport> {
    let mut report = MoveReport::default();
    for id in ids {
        let Some(rec) = db.get_file(*id)? else {
            report.failed += 1;
            continue;
        };
        let src = PathBuf::from(&rec.path);
        if !src.is_file() {
            report.failed += 1;
            continue;
        }
        let dest = unique_dest(quarantine_root, &src)?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
        }
        match relocate(&src, &dest) {
            Ok(()) => {
                db.record_quarantine(*id, &rec.path, &dest.to_string_lossy(), batch)?;
                db.set_status(*id, "quarantined")?;
                report.moved += 1;
                report.bytes += rec.size as u64;
            }
            Err(_) => report.failed += 1,
        }
    }
    Ok(report)
}

fn relocate(src: &Path, dest: &Path) -> Result<()> {
    if same_volume(src, dest) {
        std::fs::rename(src, dest).map_err(|e| AppError::io(dest, e))?;
        return Ok(());
    }
    let before = crate::hashing::hash_file(src)?;
    std::fs::copy(src, dest).map_err(|e| AppError::io(dest, e))?;
    let after = crate::hashing::hash_file(dest)?;
    if before != after {
        let _ = std::fs::remove_file(dest);
        return Err(AppError::Other(format!(
            "跨盘复制校验失败，已保留原文件: {}",
            src.display()
        )));
    }
    std::fs::remove_file(src).map_err(|e| AppError::io(src, e))?;
    Ok(())
}

fn same_volume(a: &Path, b: &Path) -> bool {
    volume_of(a) == volume_of(b)
}

fn volume_of(p: &Path) -> Option<String> {
    let abs = std::fs::canonicalize(p)
        .ok()
        .unwrap_or_else(|| p.to_path_buf());
    abs.components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase())
}

/// 隔离区里若已有同名文件，追加 `__1`、`__2`… 直到不冲突。
fn unique_dest(root: &Path, src: &Path) -> Result<PathBuf> {
    let name = src
        .file_name()
        .ok_or_else(|| AppError::Other("无文件名".into()))?;
    let mut dest = root.join(name);
    let mut n = 1;
    while dest.exists() {
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("");
        let candidate = if ext.is_empty() {
            format!("{stem}__{n}")
        } else {
            format!("{stem}__{n}.{ext}")
        };
        dest = root.join(candidate);
        n += 1;
    }
    Ok(dest)
}

/// 把整批搬回原位置。
pub fn restore(db: &Db, batch: &str) -> Result<MoveReport> {
    let rows = db.quarantine_batch(batch)?;
    let mut report = MoveReport::default();
    for (id, original, moved_path) in rows {
        let from = PathBuf::from(&moved_path);
        let to = PathBuf::from(&original);
        if !from.is_file() {
            report.failed += 1;
            continue;
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
        }
        match relocate(&from, &to) {
            Ok(()) => {
                db.mark_restored(id, crate::db::now_secs())?;
                db.set_status(id, "normal")?;
                report.moved += 1;
            }
            Err(_) => report.failed += 1,
        }
    }
    Ok(report)
}

/// 彻底删除整批隔离文件，并写入 delete_log。
pub fn purge(db: &Db, batch: &str) -> Result<PurgeReport> {
    let rows = db.quarantine_batch(batch)?;
    let mut report = PurgeReport::default();
    for (id, _original, moved_path) in rows {
        let p = PathBuf::from(&moved_path);
        let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        if p.exists() {
            std::fs::remove_file(&p).map_err(|e| AppError::io(&p, e))?;
        }
        db.log_delete(&moved_path, size as i64, batch)?;
        db.delete_quarantine_row(id)?;
        report.purged += 1;
        report.bytes += size;
    }
    Ok(report)
}

/// 一次清空整个隔离区（所有批次）。
///
/// 和单批清空一样是**永久删除**，调用方必须先做二次确认并写明数量与释放空间。
pub fn purge_all(db: &Db) -> Result<PurgeReport> {
    let batches = db.quarantine_batches()?;
    let mut report = PurgeReport::default();
    for b in batches {
        let r = purge(db, &b.batch_id)?;
        report.purged += r.purged;
        report.bytes += r.bytes;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::model::FileRecord;

    fn fixture() -> (tempfile::TempDir, tempfile::TempDir, Db, i64) {
        let src = tempfile::tempdir().unwrap();
        let q = tempfile::tempdir().unwrap();
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        let p = src.path().join("victim.jpg");
        std::fs::write(&p, b"pretend jpeg bytes").unwrap();
        let id = db
            .upsert_file(&FileRecord {
                path: p.to_string_lossy().to_string(),
                root: src.path().to_string_lossy().to_string(),
                size: 18,
                mtime: 1,
                ..Default::default()
            })
            .unwrap();
        (src, q, db, id)
    }

    #[test]
    fn purge_all_clears_every_batch() {
        let (src, q, db, id) = fixture();
        let p2 = src.path().join("victim2.jpg");
        std::fs::write(&p2, b"pretend jpeg bytes 2").unwrap();
        let id2 = db
            .upsert_file(&FileRecord {
                path: p2.to_string_lossy().to_string(),
                root: src.path().to_string_lossy().to_string(),
                size: 20,
                mtime: 1,
                ..Default::default()
            })
            .unwrap();

        move_in(&db, &[id], q.path(), "batch-a").unwrap();
        move_in(&db, &[id2], q.path(), "batch-b").unwrap();
        assert_eq!(db.quarantine_batches().unwrap().len(), 2);

        let report = purge_all(&db).unwrap();
        assert_eq!(report.purged, 2, "两个批次都要清掉");
        assert!(report.bytes > 0);
        assert!(
            db.quarantine_batches().unwrap().is_empty(),
            "清完后不该还有批次"
        );
        // 磁盘上的隔离文件也要真的没了
        let left = std::fs::read_dir(q.path()).unwrap().count();
        assert_eq!(left, 0, "隔离区目录应当空了");
    }

    #[test]
    fn move_in_relocates_file_and_records_original_path() {
        let (_src, q, db, id) = fixture();
        let rec = db.get_file(id).unwrap().unwrap();
        let original = rec.path.clone();

        let report = move_in(&db, &[id], q.path(), "batch-1").unwrap();
        assert_eq!(report.moved, 1);

        assert!(!std::path::Path::new(&original).exists(), "原位置应已清空");
        let rows = db.quarantine_batch("batch-1").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, original, "必须记住原始路径");
        assert!(std::path::Path::new(&rows[0].2).exists(), "隔离区里应存在");
        assert_eq!(db.get_file(id).unwrap().unwrap().status, "quarantined");
    }

    #[test]
    fn restore_returns_file_to_original_path() {
        let (_src, q, db, id) = fixture();
        let original = db.get_file(id).unwrap().unwrap().path.clone();

        move_in(&db, &[id], q.path(), "batch-1").unwrap();
        let report = restore(&db, "batch-1").unwrap();
        assert_eq!(report.moved, 1);

        assert!(std::path::Path::new(&original).exists(), "应回到原位");
        assert_eq!(
            std::fs::read(&original).unwrap(),
            b"pretend jpeg bytes",
            "内容必须一字不差"
        );
        assert_eq!(db.get_file(id).unwrap().unwrap().status, "normal");
    }

    #[test]
    fn purge_deletes_and_writes_log() {
        let (_src, q, db, id) = fixture();
        move_in(&db, &[id], q.path(), "batch-1").unwrap();
        let report = purge(&db, "batch-1").unwrap();
        assert_eq!(report.purged, 1);
        assert!(db.quarantine_batch("batch-1").unwrap().is_empty());

        let logged: i64 = db
            .query_column("SELECT COUNT(*) FROM delete_log", rusqlite::params![])
            .unwrap()[0];
        assert_eq!(logged, 1, "清空必须留痕");
    }

    #[test]
    fn move_in_refuses_when_source_missing() {
        let (_src, q, db, id) = fixture();
        let rec = db.get_file(id).unwrap().unwrap();
        std::fs::remove_file(&rec.path).unwrap();
        let report = move_in(&db, &[id], q.path(), "batch-x").unwrap();
        assert_eq!(report.moved, 0);
        assert_eq!(report.failed, 1, "源文件不存在应记为失败，而不是 panic");
    }

    #[test]
    fn name_collision_gets_unique_suffix() {
        let (_src, q, db, id) = fixture();
        move_in(&db, &[id], q.path(), "b1").unwrap();
        // 再放一个同名文件，移入时不能覆盖已有的那份
        let rec = db.get_file(id).unwrap().unwrap();
        std::fs::write(&rec.path, b"second version").unwrap();
        db.set_status(id, "normal").unwrap();
        move_in(&db, &[id], q.path(), "b2").unwrap();
        let all: Vec<_> = std::fs::read_dir(q.path()).unwrap().flatten().collect();
        assert_eq!(all.len(), 2, "同名冲突应各自保留");
    }
}
