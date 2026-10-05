use crate::error::{AppError, Result};
use crate::model::FileRecord;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS files (
  id                INTEGER PRIMARY KEY,
  path              TEXT NOT NULL UNIQUE,
  root              TEXT NOT NULL,
  size              INTEGER NOT NULL,
  mtime             INTEGER NOT NULL,
  ext               TEXT,
  format            TEXT,
  width             INTEGER,
  height            INTEGER,
  short_side        INTEGER,
  pid               INTEGER,
  artist            TEXT,
  content_hash      TEXT,
  phash             TEXT,
  gray_score        REAL,
  decode_error      TEXT,
  scanned_at        INTEGER,
  fingerprinted_at  INTEGER,
  status            TEXT NOT NULL DEFAULT 'normal'
);
CREATE INDEX IF NOT EXISTS idx_files_size       ON files(size);
CREATE INDEX IF NOT EXISTS idx_files_short_side ON files(short_side);
CREATE INDEX IF NOT EXISTS idx_files_content    ON files(content_hash);
CREATE INDEX IF NOT EXISTS idx_files_phash      ON files(phash);
CREATE INDEX IF NOT EXISTS idx_files_pid        ON files(pid);
-- 曾经给 (status, short_side) 加过复合索引想让「命中数」更快，结果适得其反：
-- 查询是 `... ORDER BY size DESC LIMIT 300`，有了这个过滤索引后 SQLite 会先按 short_side
-- 取出一大片再排序，丢掉了原本「按 size 有序扫描、凑够 300 条就停」的路子，
-- 列表查询从 3ms 涨到 1.7s。所以这里保持最小索引集，不加复合索引。
DROP INDEX IF EXISTS idx_files_status_size;

CREATE TABLE IF NOT EXISTS dup_groups (
  id           INTEGER PRIMARY KEY,
  kind         TEXT NOT NULL,
  keep_file_id INTEGER,
  created_at   INTEGER
);
CREATE TABLE IF NOT EXISTS dup_members (
  group_id INTEGER NOT NULL,
  file_id  INTEGER NOT NULL,
  distance INTEGER,
  PRIMARY KEY (group_id, file_id)
);

CREATE TABLE IF NOT EXISTS quarantine (
  id            INTEGER PRIMARY KEY,
  file_id       INTEGER NOT NULL,
  original_path TEXT NOT NULL,
  moved_path    TEXT NOT NULL,
  batch_id      TEXT NOT NULL,
  moved_at      INTEGER,
  restored_at   INTEGER
);

CREATE TABLE IF NOT EXISTS scan_roots (
  id       INTEGER PRIMARY KEY,
  path     TEXT NOT NULL UNIQUE,
  added_at INTEGER
);

CREATE TABLE IF NOT EXISTS delete_log (
  id        INTEGER PRIMARY KEY,
  path      TEXT NOT NULL,
  size      INTEGER,
  purged_at INTEGER,
  batch_id  TEXT
);

CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
"#;

pub struct Db {
    conn: Mutex<Connection>,
}

const UPSERT_FILE_SQL: &str = r#"INSERT INTO files
   (path, root, size, mtime, ext, format, width, height, short_side, pid, artist,
    scanned_at, content_hash, phash, gray_score, decode_error, fingerprinted_at, status)
   VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
   ON CONFLICT(path) DO UPDATE SET
     root=excluded.root, size=excluded.size, mtime=excluded.mtime,
     ext=excluded.ext, format=excluded.format, width=excluded.width,
     height=excluded.height, short_side=excluded.short_side,
     pid=excluded.pid, artist=excluded.artist, scanned_at=excluded.scanned_at,
     content_hash=NULL, phash=NULL, gray_score=NULL,
     decode_error=NULL, fingerprinted_at=NULL,
     status='normal'"#;

fn upsert_params(rec: &FileRecord) -> Vec<rusqlite::types::Value> {
    use rusqlite::types::Value;
    let status = if rec.status.is_empty() {
        "normal"
    } else {
        rec.status.as_str()
    };
    vec![
        Value::Text(rec.path.clone()),
        Value::Text(rec.root.clone()),
        Value::Integer(rec.size),
        Value::Integer(rec.mtime),
        rec.ext.clone().map(Value::Text).unwrap_or(Value::Null),
        rec.format.clone().map(Value::Text).unwrap_or(Value::Null),
        rec.width.map(Value::Integer).unwrap_or(Value::Null),
        rec.height.map(Value::Integer).unwrap_or(Value::Null),
        rec.compute_short_side()
            .map(Value::Integer)
            .unwrap_or(Value::Null),
        rec.pid.map(Value::Integer).unwrap_or(Value::Null),
        rec.artist.clone().map(Value::Text).unwrap_or(Value::Null),
        Value::Integer(now_secs()),
        rec.content_hash
            .clone()
            .map(Value::Text)
            .unwrap_or(Value::Null),
        rec.phash.clone().map(Value::Text).unwrap_or(Value::Null),
        rec.gray_score.map(Value::Real).unwrap_or(Value::Null),
        rec.decode_error
            .clone()
            .map(Value::Text)
            .unwrap_or(Value::Null),
        rec.fingerprinted_at
            .map(Value::Integer)
            .unwrap_or(Value::Null),
        Value::Text(status.to_string()),
    ]
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        Ok(Self {
            conn: Mutex::new(Connection::open_in_memory()?),
        })
    }

    pub fn migrate(&self) -> Result<()> {
        self.conn.lock().execute_batch(SCHEMA)?;
        Ok(())
    }

    #[cfg(test)]
    pub fn column_names(&self, table: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 写入或更新一条文件记录。
    ///
    /// 两条重要语义：
    /// 1. `status` 为空串时按 `normal` 处理，避免调用方漏填导致记录在筛选里消失。
    /// 2. 走到 ON CONFLICT（说明大小或修改时间变了，文件内容可能已不同）时，
    ///    会把旧的指纹列清空，逼着下一轮指纹重算——否则改过的图会留着旧指纹。
    /// 3. 同一条记录若被扫到，status 一律回到 `normal`：文件既然还躺在盘上，
    ///    就不该停留在 quarantined 上。否则「彻底清空后同路径又下了一张图」会永远看不见。
    pub fn upsert_file(&self, rec: &FileRecord) -> Result<i64> {
        let conn = self.conn.lock();
        conn.execute(
            UPSERT_FILE_SQL,
            rusqlite::params_from_iter(upsert_params(rec)),
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM files WHERE path = ?1",
            params![rec.path],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    /// 批量写入：整批走一个事务 + 一条预编译语句。
    ///
    /// 十几万张图逐条提交时，每条都要重新解析一遍 SQL 并单独提交，实测这一项就能吃掉
    /// 扫描的一半时间；批量化之后扫描才真正受限于读图片头。
    pub fn upsert_files(&self, recs: &[FileRecord]) -> Result<()> {
        if recs.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(UPSERT_FILE_SQL)?;
            for rec in recs {
                stmt.execute(rusqlite::params_from_iter(upsert_params(rec)))?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_file(&self, id: i64) -> Result<Option<FileRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT * FROM files WHERE id = ?1")?;
        let rec = stmt.query_row(params![id], row_to_record).optional()?;
        Ok(rec)
    }

    pub fn find_by_path(&self, path: &str) -> Result<Option<FileRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT * FROM files WHERE path = ?1")?;
        Ok(stmt.query_row(params![path], row_to_record).optional()?)
    }

    /// 返回 path -> (id, size, mtime)，供增量比对使用。
    pub fn path_fingerprints(&self) -> Result<std::collections::HashMap<String, (i64, i64, i64)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT path, id, size, mtime FROM files")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ),
            ))
        })?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let (path, triple) = row?;
            map.insert(path, triple);
        }
        Ok(map)
    }

    pub fn query_column<T: rusqlite::types::FromSql>(
        &self,
        sql: &str,
        p: impl rusqlite::Params,
    ) -> Result<Vec<T>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(p, |r| r.get::<_, T>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn query_files_by_size(&self, size: i64) -> Result<Vec<FileRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT * FROM files WHERE size = ?1 AND status='normal'")?;
        let rows = stmt.query_map(params![size], row_to_record)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn set_content_hash(&self, id: i64, hex: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE files SET content_hash = ?1 WHERE id = ?2",
            params![hex, id],
        )?;
        Ok(())
    }

    pub fn set_content_hash_many(&self, rows: &[(i64, String)]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached("UPDATE files SET content_hash = ?1 WHERE id = ?2")?;
            for (id, hex) in rows {
                stmt.execute(params![hex, id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn files_needing_visual(&self) -> Result<Vec<FileRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT * FROM files WHERE status='normal' AND phash IS NULL AND decode_error IS NULL",
        )?;
        let rows = stmt.query_map([], row_to_record)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn set_visual(&self, id: i64, phash_hex: &str, gray: f64, at: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE files SET phash=?1, gray_score=?2, fingerprinted_at=?3 WHERE id=?4",
            params![phash_hex, gray, at, id],
        )?;
        Ok(())
    }

    /// 批量写视觉指纹，整批一个事务（理由同 `upsert_files`）。
    pub fn set_visual_many(&self, rows: &[(i64, String, f64)], at: i64) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "UPDATE files SET phash=?1, gray_score=?2, fingerprinted_at=?3 WHERE id=?4",
            )?;
            for (id, hex, gray) in rows {
                stmt.execute(params![hex, gray, at, id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_decode_error(&self, id: i64, msg: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE files SET decode_error=?1 WHERE id=?2",
            params![msg, id],
        )?;
        Ok(())
    }

    pub fn set_decode_error_many(&self, rows: &[(i64, String)]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached("UPDATE files SET decode_error=?1 WHERE id=?2")?;
            for (id, msg) in rows {
                stmt.execute(params![msg, id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn count_visual_done(&self) -> Result<u64> {
        let conn = self.conn.lock();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM files WHERE phash IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    pub fn files_by_content_hash(&self, h: &str) -> Result<Vec<FileRecord>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT * FROM files WHERE content_hash=?1 AND status='normal'")?;
        let rows = stmt.query_map(params![h], row_to_record)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 所有已算出 pHash 的记录（完整行，供相似聚类使用）。
    pub fn files_with_phash(&self) -> Result<Vec<FileRecord>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT * FROM files WHERE status='normal' AND phash IS NOT NULL")?;
        let rows = stmt.query_map([], row_to_record)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn clear_groups(&self, kind: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM dup_members WHERE group_id IN (SELECT id FROM dup_groups WHERE kind=?1)",
            params![kind],
        )?;
        conn.execute("DELETE FROM dup_groups WHERE kind=?1", params![kind])?;
        Ok(())
    }

    pub fn insert_group(&self, kind: &str, keep: Option<i64>) -> Result<i64> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO dup_groups (kind, keep_file_id, created_at) VALUES (?1,?2,?3)",
            params![kind, keep, now_secs()],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn insert_member(&self, group_id: i64, file_id: i64, distance: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO dup_members (group_id, file_id, distance) VALUES (?1,?2,?3)",
            params![group_id, file_id, distance],
        )?;
        Ok(())
    }

    pub fn query_records(
        &self,
        sql: &str,
        args: Vec<rusqlite::types::Value>,
    ) -> Result<Vec<FileRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), row_to_record)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 界面用的分组视图。注意：先取完组列表并**释放连接锁**，再逐个查成员，
    /// 否则会在这把非可重入的锁上自锁。
    pub fn list_groups(
        &self,
        kind: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<crate::model::GroupView>> {
        let pairs: Vec<(i64, i64)> = {
            let conn = self.conn.lock();
            let mut stmt = conn.prepare(
                "SELECT id, keep_file_id FROM dup_groups WHERE kind=?1 ORDER BY id LIMIT ?2 OFFSET ?3",
            )?;
            let rows = stmt.query_map(params![kind, limit.max(1), offset.max(0)], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
            })?;
            let mut v = Vec::new();
            for row in rows {
                let (gid, keep_id) = row?;
                let keep_id = keep_id
                    .ok_or_else(|| crate::error::AppError::Schema("分组缺少保留项".into()))?;
                v.push((gid, keep_id));
            }
            v
        };

        let mut out = Vec::new();
        for (gid, keep_id) in pairs {
            let Some(keep) = self.get_file(keep_id)? else {
                continue;
            };
            let rows = self.group_member_rows(gid)?;
            let distances = rows.iter().map(|(_, d)| *d).collect();
            let members = rows.into_iter().map(|(r, _)| r).collect();
            out.push(crate::model::GroupView {
                group_id: gid,
                kind: kind.to_string(),
                keep,
                members,
                distances,
            });
        }
        Ok(out)
    }

    /// 组内除保留项以外的成员，连带各自到基准的距离，按距离升序。
    fn group_member_rows(&self, group_id: i64) -> Result<Vec<(FileRecord, i64)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT f.*, m.distance FROM dup_members m
             JOIN files f ON f.id = m.file_id
             JOIN dup_groups g ON g.id = m.group_id
             WHERE m.group_id = ?1 AND m.file_id != g.keep_file_id
             ORDER BY m.distance ASC, f.id ASC",
        )?;
        let rows = stmt.query_map(params![group_id], |r| {
            Ok((
                row_to_record(r)?,
                r.get::<_, Option<i64>>("distance")?.unwrap_or(0),
            ))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn set_group_keeper(&self, group_id: i64, file_id: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE dup_groups SET keep_file_id=?1 WHERE id=?2",
            params![file_id, group_id],
        )?;
        Ok(())
    }

    pub fn record_quarantine(
        &self,
        file_id: i64,
        original: &str,
        moved: &str,
        batch: &str,
    ) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO quarantine (file_id, original_path, moved_path, batch_id, moved_at)
             VALUES (?1,?2,?3,?4,?5)",
            params![file_id, original, moved, batch, now_secs()],
        )?;
        Ok(())
    }

    pub fn set_status(&self, id: i64, status: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE files SET status=?1 WHERE id=?2",
            params![status, id],
        )?;
        Ok(())
    }

    pub fn quarantine_batch(&self, batch: &str) -> Result<Vec<(i64, String, String)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT file_id, original_path, moved_path FROM quarantine
             WHERE batch_id=?1 AND restored_at IS NULL",
        )?;
        let rows = stmt.query_map(params![batch], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn mark_restored(&self, file_id: i64, at: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE quarantine SET restored_at=?1 WHERE file_id=?2 AND restored_at IS NULL",
            params![at, file_id],
        )?;
        Ok(())
    }

    pub fn log_delete(&self, path: &str, size: i64, batch: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO delete_log (path, size, purged_at, batch_id) VALUES (?1,?2,?3,?4)",
            params![path, size, now_secs(), batch],
        )?;
        Ok(())
    }

    pub fn delete_quarantine_row(&self, file_id: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM quarantine WHERE file_id=?1", params![file_id])?;
        Ok(())
    }

    /// 隔离区里还没搬回的批次，按最近移入排序，带张数与体积。
    pub fn quarantine_batches(&self) -> Result<Vec<crate::model::QuarantineBatch>> {
        let heads: Vec<(String, i64, i64)> = {
            let conn = self.conn.lock();
            let mut stmt = conn.prepare(
                "SELECT batch_id, COUNT(*), MAX(moved_at) FROM quarantine
                 WHERE restored_at IS NULL GROUP BY batch_id ORDER BY MAX(moved_at) DESC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        let mut out = Vec::new();
        for (batch_id, count, moved_at) in heads {
            let bytes: i64 = {
                let conn = self.conn.lock();
                conn.query_row(
                    "SELECT COALESCE(SUM(f.size),0) FROM quarantine q JOIN files f ON f.id=q.file_id
                     WHERE q.batch_id=?1 AND q.restored_at IS NULL",
                    params![batch_id],
                    |r| r.get(0),
                )?
            };
            out.push(crate::model::QuarantineBatch {
                batch_id,
                count,
                bytes,
                moved_at,
            });
        }
        Ok(out)
    }

    pub fn query_count(&self, sql: &str, args: Vec<rusqlite::types::Value>) -> Result<i64> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(sql)?;
        Ok(stmt.query_row(rusqlite::params_from_iter(args), |r| r.get(0))?)
    }
}

fn row_to_record(r: &rusqlite::Row<'_>) -> rusqlite::Result<FileRecord> {
    Ok(FileRecord {
        id: r.get("id")?,
        path: r.get("path")?,
        root: r.get("root")?,
        size: r.get("size")?,
        mtime: r.get("mtime")?,
        ext: r.get("ext")?,
        format: r.get("format")?,
        width: r.get("width")?,
        height: r.get("height")?,
        short_side: r.get("short_side")?,
        pid: r.get("pid")?,
        artist: r.get("artist")?,
        content_hash: r.get("content_hash")?,
        phash: r.get("phash")?,
        gray_score: r.get("gray_score")?,
        decode_error: r.get("decode_error")?,
        scanned_at: r.get("scanned_at")?,
        fingerprinted_at: r.get("fingerprinted_at")?,
        status: r.get("status")?,
    })
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_files_table_with_short_side() {
        let db = Db::open_in_memory().expect("open");
        db.migrate().expect("migrate");
        let cols = db.column_names("files").expect("columns");
        assert!(
            cols.contains(&"short_side".to_string()),
            "缺 short_side 列: {cols:?}"
        );
        assert!(cols.contains(&"content_hash".to_string()));
        assert!(cols.contains(&"phash".to_string()));
        assert!(cols.contains(&"gray_score".to_string()));
    }

    #[test]
    fn migrate_is_idempotent() {
        let db = Db::open_in_memory().expect("open");
        db.migrate().expect("first");
        db.migrate().expect("second");
    }

    #[test]
    fn insert_and_fetch_file() {
        let db = Db::open_in_memory().expect("open");
        db.migrate().expect("migrate");
        let rec = FileRecord {
            path: r"D:\色图\a.jpg".into(),
            root: r"D:\色图".into(),
            size: 1024,
            mtime: 100,
            ext: Some("jpg".into()),
            width: Some(800),
            height: Some(600),
            ..Default::default()
        };
        let id = db.upsert_file(&rec).expect("upsert");
        let got = db.get_file(id).expect("get").expect("some");
        assert_eq!(got.short_side, Some(600), "short_side 应由写入时算好");
    }

    /// 文件被扫到时，记录必须回到 normal 并且旧指纹清空。
    /// 破坏性端到端验证发现了这个漏洞：彻底清空后同路径再放一张图，图库永远看不到它。
    #[test]
    fn upsert_revives_status_and_clears_stale_fingerprints() {
        let db = Db::open_in_memory().expect("open");
        db.migrate().expect("migrate");
        let rec = FileRecord {
            path: r"D:\色图\a.jpg".into(),
            root: r"D:\色图".into(),
            size: 1024,
            mtime: 100,
            content_hash: Some("deadbeef".into()),
            phash: Some("0000000000000001".into()),
            gray_score: Some(3.0),
            ..Default::default()
        };
        let id = db.upsert_file(&rec).expect("upsert");
        db.set_status(id, "quarantined").expect("status");

        let changed = FileRecord {
            size: 2048,
            mtime: 200,
            ..rec.clone()
        };
        db.upsert_file(&changed).expect("upsert again");

        let got = db.get_file(id).expect("get").expect("some");
        assert_eq!(got.status, "normal", "文件还在盘上就不该停在 quarantined");
        assert!(got.content_hash.is_none(), "内容变了，旧内容指纹必须作废");
        assert!(got.phash.is_none(), "内容变了，旧视觉指纹必须作废");
        assert!(got.gray_score.is_none());
    }
}
