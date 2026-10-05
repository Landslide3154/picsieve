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

    pub fn upsert_file(&self, rec: &FileRecord) -> Result<i64> {
        let short_side = rec.compute_short_side();
        let now = now_secs();
        let conn = self.conn.lock();
        conn.execute(
            r#"INSERT INTO files
               (path, root, size, mtime, ext, format, width, height, short_side, pid, artist, scanned_at)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
               ON CONFLICT(path) DO UPDATE SET
                 root=excluded.root, size=excluded.size, mtime=excluded.mtime,
                 ext=excluded.ext, format=excluded.format, width=excluded.width,
                 height=excluded.height, short_side=excluded.short_side,
                 pid=excluded.pid, artist=excluded.artist, scanned_at=excluded.scanned_at"#,
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
                now
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

    /// 返回 path -> (id, size, mtime)，供增量比对使用。
    pub fn path_fingerprints(&self) -> Result<std::collections::HashMap<String, (i64, i64, i64)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT path, id, size, mtime FROM files")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?),
            ))
        })?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let (path, triple) = row?;
            map.insert(path, triple);
        }
        Ok(map)
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
