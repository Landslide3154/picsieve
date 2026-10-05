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
    ///    但**不动 status**：已进隔离区的记录不能被一次扫描复活。
    pub fn upsert_file(&self, rec: &FileRecord) -> Result<i64> {
        let short_side = rec.compute_short_side();
        let now = now_secs();
        let status = if rec.status.is_empty() {
            "normal"
        } else {
            rec.status.as_str()
        };
        let conn = self.conn.lock();
        conn.execute(
            r#"INSERT INTO files
               (path, root, size, mtime, ext, format, width, height, short_side, pid, artist,
                scanned_at, content_hash, phash, gray_score, decode_error, fingerprinted_at, status)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
               ON CONFLICT(path) DO UPDATE SET
                 root=excluded.root, size=excluded.size, mtime=excluded.mtime,
                 ext=excluded.ext, format=excluded.format, width=excluded.width,
                 height=excluded.height, short_side=excluded.short_side,
                 pid=excluded.pid, artist=excluded.artist, scanned_at=excluded.scanned_at,
                 content_hash=NULL, phash=NULL, gray_score=NULL,
                 decode_error=NULL, fingerprinted_at=NULL"#,
            params![
                rec.path,
                rec.root,
                rec.size,
                rec.mtime,
                rec.ext,
                rec.format,
                rec.width,
                rec.height,
                short_side,
                rec.pid,
                rec.artist,
                now,
                rec.content_hash,
                rec.phash,
                rec.gray_score,
                rec.decode_error,
                rec.fingerprinted_at,
                status
            ],
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM files WHERE path = ?1",
            params![rec.path],
            |r| r.get(0),
        )?;
        Ok(id)
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

    pub fn set_decode_error(&self, id: i64, msg: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE files SET decode_error=?1 WHERE id=?2",
            params![msg, id],
        )?;
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

    pub fn files_with_phash(&self) -> Result<Vec<crate::grouper::VisualRec>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, phash, pid, path FROM files WHERE status='normal' AND phash IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            let id: i64 = r.get(0)?;
            let hex: String = r.get(1)?;
            let pid: Option<i64> = r.get(2)?;
            let path: String = r.get(3)?;
            let stem = std::path::Path::new(&path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            Ok((id, hex, pid, stem))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, hex, pid, stem) = row?;
            if let Some(phash) = crate::phash::from_hex(&hex) {
                out.push(crate::grouper::VisualRec {
                    id,
                    phash,
                    pid,
                    page: crate::grouper::page_of(&stem),
                });
            }
        }
        Ok(out)
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
}
