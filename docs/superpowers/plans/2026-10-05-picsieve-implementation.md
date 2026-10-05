# 图筛 PicSieve 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 构建 Windows 桌面工具「图筛 PicSieve」，对本机 13.7 万张 Pixiv 图片做重复检测、条件筛选与安全清理。

**架构：** Tauri 2 桌面应用。Rust 后端负责目录扫描、内容指纹（BLAKE3）、视觉指纹（pHash）、灰度判定、SQLite 持久化、重复分组与隔离区文件移动；Vue 3 + TypeScript 前端负责图库网格、筛选栏、重复组审阅。前后端通过 Tauri 命令（invoke）与事件（emit）通信。

**技术栈：** Rust 2021 / Tauri 2 / rusqlite（bundled SQLite）/ image / blake3 / rayon / jwalk / regex / thiserror / Vue 3 / TypeScript / Vite / pnpm

**规格来源：** [`docs/superpowers/specs/2026-10-05-picsieve-design.md`](../specs/2026-10-05-picsieve-design.md)

**本机已验证的工具链：** rustc 1.99.0 (x86_64-pc-windows-msvc)、cargo 1.99.0、MSVC 2022 Community（`C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC`）、WebView2 154.0.4258.53、node v24.14.0、pnpm 11.22.0。

**贯穿全程的约定：**

- 一律在 `D:\code\PicSieve` 下操作；git 身份 `Developer <dev@picsieve.local>`。
- 每个任务结束时必须 commit 并 push 到 `origin main`（用户明确要求远端保持同步）。
- 命令用 Git Bash 风格路径（`/d/code/PicSieve`），或用 `workdir` 参数指定工作目录。
- Rust 代码一律 `cargo fmt` + `cargo clippy -- -D warnings` 后再提交。
- 前端一律 `pnpm build`（含 `vue-tsc` 类型检查）通过后再提交。
- 测试图片一律用程序生成，**绝不读取用户真实图片目录做单元测试**。

---

## 文件结构

先锁定分解，再拆任务。

```
D:\code\PicSieve\
├── package.json                  前端依赖与脚本
├── pnpm-lock.yaml
├── index.html                    Vite 入口 HTML
├── vite.config.ts                Vite 配置（Tauri 专用端口与 HMR）
├── tsconfig.json                 TS 配置
├── .gitignore
│
├── src/                          ── 前端（Vue 3）
│   ├── main.ts                   挂载 App
│   ├── App.vue                   顶层布局与视图切换
│   ├── types.ts                  与 Rust 侧对应的 TS 类型
│   ├── api.ts                    所有 invoke/event 调用的唯一封装处
│   ├── stores/
│   │   └── library.ts            图库状态（筛选条件、选中集合、结果分页）
│   ├── components/
│   │   ├── ScanProgress.vue      扫描与指纹进度
│   │   ├── FilterPanel.vue       左侧筛选栏
│   │   ├── ThumbGrid.vue         虚拟滚动缩略图网格
│   │   ├── ThumbCell.vue         单格缩略图（含 ×N / 灰 角标）
│   │   ├── ActionBar.vue         底部已选数量与操作
│   │   ├── DupGroupView.vue      重复组审阅
│   │   └── QuarantineView.vue    隔离区管理
│   └── styles/main.css           全局样式与主题变量
│
└── src-tauri/                    ── 后端（Rust）
    ├── Cargo.toml
    ├── build.rs
    ├── tauri.conf.json           窗口、打包、CSP 配置
    ├── capabilities/default.json Tauri 2 权限声明
    ├── icons/                    应用图标
    └── src/
        ├── main.rs               进程入口，调用 lib::run()
        ├── lib.rs                模块声明 + Tauri Builder 组装
        ├── error.rs              AppError / Result
        ├── model.rs              FileRecord / Filter / SortKey / 各类报告结构
        ├── db.rs                 SQLite 连接、建表、迁移
        ├── nameparse.rs          从文件名提取作品 ID 与画师名
        ├── scanner.rs            目录遍历、读图片头、增量入库
        ├── hashing.rs            内容指纹（大小预筛 + BLAKE3）
        ├── phash.rs              64 位感知哈希 + 汉明距离
        ├── gray.rs               灰度分数
        ├── fingerprint.rs        指纹调度：并行、断点续算
        ├── grouper.rs            完全重复分组、相似分组、保留规则
        ├── query.rs              条件筛选查询
        ├── thumb.rs              缩略图缓存
        ├── quarantine.rs         移入隔离区 / 搬回 / 彻底清空
        ├── settings.rs           设置读写（线程数、阈值、目录）
        └── commands.rs           Tauri 命令与进度事件
```

**职责边界：** `commands.rs` 只做参数校验与转发，不含业务逻辑；所有业务逻辑在各自的领域模块里，且都能用纯 Rust 单元测试覆盖。前端 `api.ts` 是唯一与后端通话的地方，组件不直接 `invoke`。

---

## 里程碑与任务索引

| 里程碑 | 任务 | 交付物 |
|---|---|---|
| M1 扫描链路 | 1–6 | 能选目录、扫描入库、界面显示进度与真实总数 |
| M2 指纹 | 7–10 | 每张图有内容指纹、视觉指纹、灰度分数，可断点续算 |
| M3 分组与查询 | 11–13 | 能查出重复组，能按条件筛选 |
| M4 界面 | 14–16 | 图库网格、筛选栏、重复组审阅可用 |
| M5 隔离区与设置 | 17–19 | 能移入、搬回、清空；各项参数可调 |
| M6 打包 | 20 | 安装包与免安装包，真机冒烟 |

---

## 任务 1：项目骨架

**文件：**
- 创建：`package.json`、`index.html`、`vite.config.ts`、`tsconfig.json`、`tsconfig.node.json`
- 创建：`src/main.ts`、`src/App.vue`、`src/styles/main.css`
- 创建：`src-tauri/Cargo.toml`、`src-tauri/build.rs`、`src-tauri/tauri.conf.json`、`src-tauri/capabilities/default.json`
- 创建：`src-tauri/src/main.rs`、`src-tauri/src/lib.rs`

- [ ] **步骤 1：用官方脚手架生成到临时目录**

在 `D:\code` 下执行（不要直接在 `PicSieve` 目录里生成，那里已有 `docs/` 和 `.git/`）：

```bash
cd /d/code
pnpm create tauri-app@latest picsieve-scaffold -t vue-ts -m pnpm -y
```

预期：生成 `D:\code\picsieve-scaffold\`，包含 `src/`、`src-tauri/`、`package.json`。

- [ ] **步骤 2：把脚手架内容搬进项目目录**

```bash
cd /d/code/picsieve-scaffold
cp -r package.json index.html vite.config.ts tsconfig.json tsconfig.node.json src src-tauri /d/code/PicSieve/
cd /d/code
rm -rf picsieve-scaffold
```

注意：**不要**覆盖 `D:\code\PicSieve\.git`、`docs`、`.gitignore`。

- [ ] **步骤 3：确认 .gitignore 覆盖脚手架产物**

`D:\code\PicSieve\.gitignore` 追加：

```
src-tauri/target/
src-tauri/gen/
```

- [ ] **步骤 4：安装前端依赖**

```bash
cd /d/code/PicSieve
pnpm install
```

预期：`node_modules/` 生成，无错误。

- [ ] **步骤 5：确认能编译并起窗口**

```bash
cd /d/code/PicSieve
pnpm tauri dev
```

预期：出现一个桌面窗口，显示脚手架自带的示例页面。看到窗口后按 `Ctrl+C` 退出。

首次编译 Rust 依赖需要几分钟，属正常。

- [ ] **步骤 6：Commit 并 push**

```bash
cd /d/code/PicSieve
git add -A
git commit -m "chore: Tauri 2 + Vue 3 项目骨架"
git push origin main
```

---

## 任务 2：数据库层（error / model / db）

**文件：**
- 创建：`src-tauri/src/error.rs`、`src-tauri/src/model.rs`、`src-tauri/src/db.rs`
- 修改：`src-tauri/src/lib.rs`（声明模块）

- [ ] **步骤 1：添加 Rust 依赖**

```bash
cd /d/code/PicSieve/src-tauri
cargo add rusqlite --features bundled
cargo add serde --features derive
cargo add serde_json
cargo add thiserror
cargo add parking_lot
cargo add tempfile --dev
```

- [ ] **步骤 2：编写失败的测试**

在 `src-tauri/src/db.rs` 末尾写：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_files_table_with_short_side() {
        let db = Db::open_in_memory().expect("open");
        db.migrate().expect("migrate");
        let cols = db.column_names("files").expect("columns");
        assert!(cols.contains(&"short_side".to_string()), "缺 short_side 列: {cols:?}");
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
```

- [ ] **步骤 3：运行测试验证失败**

```bash
cd /d/code/PicSieve/src-tauri
cargo test db:: 2>&1 | tail -20
```

预期：编译失败，报 `cannot find type Db` / `FileRecord` 未定义。

- [ ] **步骤 4：实现 error.rs**

```rust
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("文件读写失败 {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("图片解码失败 {path}: {source}")]
    Image {
        path: PathBuf,
        #[source]
        source: image::ImageError,
    },

    #[error("路径不是目录: {0}")]
    NotADirectory(PathBuf),

    #[error("磁盘空间不足：需要 {need} 字节，可用 {available} 字节")]
    InsufficientSpace { need: u64, available: u64 },

    #[error("数据库结构异常: {0}")]
    Schema(String),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        AppError::Io { path: path.into(), source }
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
```

- [ ] **步骤 5：实现 model.rs**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub id: i64,
    pub path: String,
    pub root: String,
    pub size: i64,
    pub mtime: i64,
    pub ext: Option<String>,
    pub format: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub short_side: Option<i64>,
    pub pid: Option<i64>,
    pub artist: Option<String>,
    pub content_hash: Option<String>,
    pub phash: Option<String>,
    pub gray_score: Option<f64>,
    pub decode_error: Option<String>,
    pub scanned_at: Option<i64>,
    pub fingerprinted_at: Option<i64>,
    pub status: String,
}

impl FileRecord {
    /// 供索引使用的 short_side；宽或高未知时返回 None。
    pub fn compute_short_side(&self) -> Option<i64> {
        self.width.zip(self.height).map(|(w, h)| w.min(h))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortKey {
    #[default]
    SizeDesc,
    SizeAsc,
    ShortSideDesc,
    ShortSideAsc,
    PathAsc,
}

impl SortKey {
    /// 拼进 SQL 的 ORDER BY 片段。取值全部来自本枚举，不含用户输入。
    pub fn order_clause(self) -> &'static str {
        match self {
            SortKey::SizeDesc => "size DESC",
            SortKey::SizeAsc => "size ASC",
            SortKey::ShortSideDesc => "short_side DESC NULLS LAST",
            SortKey::ShortSideAsc => "short_side ASC NULLS LAST",
            SortKey::PathAsc => "path ASC",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub min_short_side: Option<i64>,
    pub max_short_side: Option<i64>,
    pub min_size: Option<i64>,
    pub max_size: Option<i64>,
    pub exts: Vec<String>,
    pub only_gray: bool,
    pub only_duplicated: bool,
    pub only_decode_error: bool,
    pub search: Option<String>,
    pub sort: SortKey,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStats {
    pub seen: u64,
    pub inserted: u64,
    pub updated: u64,
    pub skipped: u64,
    pub failed: u64,
}
```

- [ ] **步骤 6：实现 db.rs**

```rust
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
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn open_in_memory() -> Result<Self> {
        Ok(Self { conn: Mutex::new(Connection::open_in_memory()?) })
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
                rec.path, rec.root, rec.size, rec.mtime, rec.ext, rec.format,
                rec.width, rec.height, short_side, rec.pid, rec.artist, now
            ],
        )?;
        let id: i64 = conn.query_row("SELECT id FROM files WHERE path = ?1", params![rec.path], |r| r.get(0))?;
        Ok(id)
    }

    pub fn get_file(&self, id: i64) -> Result<Option<FileRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT * FROM files WHERE id = ?1")?;
        let rec = stmt.query_row(params![id], row_to_record).optional()?;
        Ok(rec)
    }

    /// 返回 (id, size, mtime)，供增量比对使用。
    pub fn path_fingerprints(&self) -> Result<std::collections::HashMap<String, (i64, i64, i64)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT path, id, size, mtime FROM files")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)))
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
```

- [ ] **步骤 7：在 lib.rs 声明模块**

```rust
pub mod db;
pub mod error;
pub mod model;
```

- [ ] **步骤 8：运行测试验证通过**

```bash
cd /d/code/PicSieve/src-tauri
cargo test db:: 2>&1 | tail -20
```

预期：3 个测试全部 PASS。

- [ ] **步骤 9：格式化、静态检查、提交**

```bash
cd /d/code/PicSieve/src-tauri
cargo fmt
cargo clippy --all-targets -- -D warnings
cd /d/code/PicSieve
git add -A
git commit -m "feat(db): SQLite 数据层与 FileRecord 模型"
git push origin main
```

---

## 任务 3：文件名解析（pid / 画师名）

**文件：**
- 创建：`src-tauri/src/nameparse.rs`
- 修改：`src-tauri/src/lib.rs`

- [ ] **步骤 1：添加依赖**

```bash
cd /d/code/PicSieve/src-tauri
cargo add regex
```

- [ ] **步骤 2：编写失败的测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // 全部取自用户真实文件名样本（规格第 2 节）
    #[test]
    fn parses_pid_with_brackets() {
        assert_eq!(parse_pid("白ウサギ - 赤倉@初画集＆個展 - [pid=115088821] -"), Some(115088821));
    }

    #[test]
    fn parses_pid_uppercase_prefix() {
        assert_eq!(parse_pid("#1 - 愛言葉Ⅴ - おむたつ／omutatsu - PID=141311340 -"), Some(141311340));
    }

    #[test]
    fn parses_pid_attached_to_title() {
        assert_eq!(parse_pid("【yae】　狼ト生キル - 成人式[pid=48119895]"), Some(48119895));
    }

    #[test]
    fn parses_bare_numeric_pid() {
        assert_eq!(parse_pid("#1 - ∞ - Rella - 133377800 -"), Some(133377800));
    }

    #[test]
    fn parses_pid_only_filename() {
        assert_eq!(parse_pid("[pid=65519461]"), Some(65519461));
    }

    #[test]
    fn returns_none_when_no_pid() {
        assert_eq!(parse_pid("-.jpg"), None);
        assert_eq!(parse_pid("_LM7_ - 105"), None, "105 太短，不应被当作作品 ID");
    }

    #[test]
    fn parses_artist_from_four_part_name() {
        assert_eq!(
            parse_artist("#1 - 白ウサギ - 赤倉@初画集＆個展 - [pid=115088821] -"),
            Some("赤倉@初画集＆個展".to_string())
        );
        assert_eq!(parse_artist("#1 - ∞ - Rella - 133377800 -"), Some("Rella".to_string()));
    }

    #[test]
    fn returns_none_when_artist_unknown() {
        assert_eq!(parse_artist("【yae】　狼ト生キル - 成人式[pid=48119895]"), None);
        assert_eq!(parse_artist("-.jpg"), None);
    }

    #[test]
    fn does_not_treat_ranking_number_as_pid() {
        // "#1" 是排行榜名次，绝不能被当成作品 ID
        assert_eq!(parse_pid("#1 - タイトル - 作者 - [pid=12345678] -"), Some(12345678));
    }
}
```

- [ ] **步骤 3：运行测试验证失败**

```bash
cd /d/code/PicSieve/src-tauri
cargo test nameparse 2>&1 | tail -20
```

预期：编译失败，`parse_pid` 未定义。

- [ ] **步骤 4：实现 nameparse.rs**

```rust
use once_cell::sync::Lazy;
use regex::Regex;

static RE_PID_TAGGED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)pid\s*=\s*(\d{5,12})").expect("pid regex"));

/// 被非数字字符（或字符串边界）包围的 5–12 位数字串。
static RE_PID_BARE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:^|[^0-9])(\d{5,12})(?:[^0-9]|$)").expect("bare pid regex"));

/// 从文件主名（不含扩展名）里提取 Pixiv 作品 ID。
///
/// 规则：优先认带 `pid=` 标记的；找不到再认被分隔符包围的 5–12 位数字串。
/// 找不到返回 None。作品 ID 只用于展示与「同作品多页」辅助判断，
/// **不参与任何自动删除决策**，因此这里宁可返回 None 也不猜。
pub fn parse_pid(stem: &str) -> Option<i64> {
    if let Some(c) = RE_PID_TAGGED.captures(stem) {
        if let Ok(v) = c[1].parse::<i64>() {
            return Some(v);
        }
    }
    RE_PID_BARE
        .captures_iter(stem)
        .filter_map(|c| c[1].parse::<i64>().ok())
        .max()
}

/// 从文件名提取画师名。
///
/// 用户文件名形如 `#名次 - 标题 - 画师 - [pid=…] -`。做法：按 ` - ` 切段，
/// 丢掉空段与含作品 ID 的段；若还剩至少 3 段，取第 3 段作为画师名。
/// 形状不符时返回 None。
pub fn parse_artist(stem: &str) -> Option<String> {
    let parts: Vec<&str> = stem
        .split(" - ")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| parse_pid(s).is_none())
        .collect();
    if parts.len() >= 3 {
        let a = parts[2];
        if a.len() <= 80 {
            return Some(a.to_string());
        }
    }
    None
}
```

- [ ] **步骤 5：添加 once_cell 依赖并运行测试**

```bash
cd /d/code/PicSieve/src-tauri
cargo add once_cell
cargo test nameparse 2>&1 | tail -25
```

预期：全部 PASS。若 `returns_none_when_no_pid` 失败（`105` 被误认），确认 `RE_PID_BARE` 用的是 `{5,12}` 而不是 `{1,12}`。

- [ ] **步骤 6：格式化、静态检查、提交**

```bash
cd /d/code/PicSieve/src-tauri
cargo fmt && cargo clippy --all-targets -- -D warnings
cd /d/code/PicSieve
git add -A
git commit -m "feat(nameparse): 从文件名提取作品 ID 与画师名"
git push origin main
```

---

## 任务 4：扫描器

**文件：**
- 创建：`src-tauri/src/scanner.rs`
- 修改：`src-tauri/src/lib.rs`

- [ ] **步骤 1：添加依赖**

```bash
cd /d/code/PicSieve/src-tauri
cargo add image --no-default-features --features jpeg,png,gif,webp,bmp
cargo add jwalk
cargo add rayon
```

- [ ] **步骤 2：编写失败的测试**

```rust
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
        let stats = scan(&db, &[root.to_path_buf()], &ScanOptions::default(), &mut |_| {}).expect("scan");

        assert_eq!(stats.inserted, 2, "两次图片应入库，txt 应被忽略");
        assert_eq!(stats.failed, 0);
        let a = db.find_by_path(&root.join("a.png").to_string_lossy()).expect("q").expect("some");
        assert_eq!((a.width, a.height, a.short_side), (Some(800), Some(600), Some(600)));
        assert_eq!(a.format.as_deref(), Some("png"));
    }

    #[test]
    fn second_scan_skips_unchanged_files() {
        let dir = tempfile::tempdir().expect("tmp");
        make_png(&dir.path().join("a.png"), 100, 100);
        let db = Db::open_in_memory().expect("db");
        db.migrate().expect("migrate");

        scan(&db, &[dir.path().to_path_buf()], &ScanOptions::default(), &mut |_| {}).expect("first");
        let second = scan(&db, &[dir.path().to_path_buf()], &ScanOptions::default(), &mut |_| {}).expect("second");

        assert_eq!(second.inserted, 0);
        assert_eq!(second.skipped, 1, "大小与修改时间都没变，应跳过");
    }

    #[test]
    fn corrupt_image_is_recorded_not_fatal() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(dir.path().join("broken.jpg"), b"\xFF\xD8\xFF\xE0garbage").expect("write");
        let db = Db::open_in_memory().expect("db");
        db.migrate().expect("migrate");
        let stats = scan(&db, &[dir.path().to_path_buf()], &ScanOptions::default(), &mut |_| {}).expect("scan");
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.inserted, 0);
    }
}
```

- [ ] **步骤 3：运行测试验证失败**

```bash
cd /d/code/PicSieve/src-tauri
cargo test scanner 2>&1 | tail -20
```

预期：编译失败，`scan` 未定义。

- [ ] **步骤 4：实现 scanner.rs**

```rust
use crate::db::{now_secs, Db};
use crate::error::{AppError, Result};
use crate::model::{FileRecord, ScanStats};
use crate::nameparse;
use rayon::prelude::*;
use std::path::{Path, PathBuf};

/// 只处理这些扩展名的文件；其余只计数不入库。
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub threads: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        let n = std::thread::available_parallelism().map(|v| v.get()).unwrap_or(4);
        Self { threads: n.saturating_sub(2).max(1) }
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

    let results: Vec<std::result::Result<Option<FileRecord>, ()>> = pool.install(|| {
        candidates
            .par_iter()
            .map(|(path, root)| -> std::result::Result<Option<FileRecord>, ()> {
                match build_record(path, root) {
                    Ok(rec) => Ok(Some(rec)),
                    Err(_) => Err(()),
                }
            })
            .collect()
    });

    let mut stats = ScanStats::default();
    let mut seen = 0u64;
    for ((path, _root), res) in candidates.iter().zip(results.into_iter()) {
        seen += 1;
        match res {
            Ok(Some(rec)) => {
                let key = path.to_string_lossy().to_string();
                let unchanged = known
                    .get(&key)
                    .map(|(_, size, mtime)| *size == rec.size && *mtime == rec.mtime)
                    .unwrap_or(false);
                if unchanged {
                    stats.skipped += 1;
                } else {
                    let existed = known.contains_key(&key);
                    db.upsert_file(&rec)?;
                    if existed {
                        stats.updated += 1;
                    } else {
                        stats.inserted += 1;
                    }
                }
            }
            _ => stats.failed += 1,
        }
        if seen % 500 == 0 || seen == total_hint {
            progress(ScanProgress {
                seen,
                total_hint,
                current: path.to_string_lossy().to_string(),
            });
        }
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

    // 只读图片头拿宽高与真实格式
    let reader = image::ImageReader::open(path)
        .map_err(|e| AppError::io(path, e))?
        .with_guessed_format()
        .map_err(|e| AppError::io(path, e))?;
    let format = reader.format().map(|f| format!("{f:?}").to_ascii_lowercase());
    let (width, height) = reader
        .into_dimensions()
        .map(|(w, h)| (Some(w as i64), Some(h as i64)))
        .map_err(|e| AppError::Image { path: path.to_path_buf(), source: e })?;

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
        decode_error: None,
        scanned_at: Some(now_secs()),
        fingerprinted_at: None,
        status: "normal".into(),
    })
}
```

- [ ] **步骤 5：在 db.rs 补一个测试用查询方法**

```rust
    pub fn find_by_path(&self, path: &str) -> Result<Option<FileRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT * FROM files WHERE path = ?1")?;
        Ok(stmt.query_row(params![path], row_to_record).optional()?)
    }
```

- [ ] **步骤 6：运行测试验证通过**

```bash
cd /d/code/PicSieve/src-tauri
cargo test scanner 2>&1 | tail -25
```

预期：3 个测试 PASS。

若 `corrupt_image_is_recorded_not_fatal` 失败，检查 `build_record` 是否在 `into_dimensions()` 报错时返回 `Err`——就应该返回 `Err`，由上层计入 `failed`。

- [ ] **步骤 7：格式化、静态检查、提交**

```bash
cd /d/code/PicSieve/src-tauri
cargo fmt && cargo clippy --all-targets -- -D warnings
cd /d/code/PicSieve
git add -A
git commit -m "feat(scanner): 目录遍历、读图片头与增量入库"
git push origin main
```

---

## 任务 5：扫描命令与进度事件（后端）

**文件：**
- 创建：`src-tauri/src/settings.rs`、`src-tauri/src/commands.rs`
- 修改：`src-tauri/src/lib.rs`、`src-tauri/Cargo.toml`

- [ ] **步骤 1：添加依赖**

```bash
cd /d/code/PicSieve/src-tauri
cargo add tauri-plugin-dialog@2
cargo add uuid --features v4
```

- [ ] **步骤 2：编写失败的测试**

在 `src-tauri/src/settings.rs` 里：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let s = Settings::default();
        let n = std::thread::available_parallelism().map(|v| v.get()).unwrap_or(4);
        assert_eq!(s.threads, n.saturating_sub(2).max(1), "默认线程数 = 逻辑核心数 - 2");
        assert_eq!(s.similar_threshold, 8);
        assert_eq!(s.gray_threshold, 8.0);
        assert_eq!(s.thumb_cache_limit_mb, 3072);
    }

    #[test]
    fn quarantine_must_not_be_inside_scan_root() {
        let roots = vec![r"D:\色图\2017-2024 PIXIV daily".to_string()];
        assert!(validate_quarantine_dir(r"D:\色图\_待确认删除", &roots).is_ok());
        assert!(validate_quarantine_dir(r"D:\色图\2017-2024 PIXIV daily\_待确认删除", &roots).is_err());
    }
}
```

- [ ] **步骤 3：运行测试验证失败**

```bash
cd /d/code/PicSieve/src-tauri
cargo test settings 2>&1 | tail -20
```

预期：编译失败，`Settings` 未定义。

- [ ] **步骤 4：实现 settings.rs**

```rust
use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub threads: usize,
    pub similar_threshold: u32,
    pub gray_threshold: f64,
    pub thumb_max_edge: u32,
    pub thumb_cache_limit_mb: u64,
    pub quarantine_dir: String,
    pub roots: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        let n = std::thread::available_parallelism().map(|v| v.get()).unwrap_or(4);
        Self {
            threads: n.saturating_sub(2).max(1),
            similar_threshold: 8,
            gray_threshold: 8.0,
            thumb_max_edge: 320,
            thumb_cache_limit_mb: 3072,
            quarantine_dir: r"D:\色图\_待确认删除".to_string(),
            roots: Vec::new(),
        }
    }
}

/// 隔离区目录不得位于任何扫描根之内或之下，否则下次扫描会把待删文件又捞回来。
pub fn validate_quarantine_dir(dir: &str, roots: &[String]) -> Result<()> {
    let q = normalize(dir);
    for r in roots {
        let rn = normalize(r);
        if q == rn || q.starts_with(&(rn.clone() + "\\")) {
            return Err(AppError::Other(format!(
                "隔离区不能放在扫描目录「{r}」里面，否则下次扫描会把它当成待处理文件"
            )));
        }
    }
    Ok(())
}

fn normalize(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_ascii_lowercase()
}

pub fn settings_path(app_data: &Path) -> PathBuf {
    app_data.join("settings.json")
}

pub fn load(app_data: &Path) -> Settings {
    let p = settings_path(app_data);
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default()
}

pub fn save(app_data: &Path, s: &Settings) -> Result<()> {
    std::fs::create_dir_all(app_data).map_err(|e| AppError::io(app_data, e))?;
    let p = settings_path(app_data);
    let text = serde_json::to_string_pretty(s).map_err(|e| AppError::Other(e.to_string()))?;
    std::fs::write(&p, text).map_err(|e| AppError::io(&p, e))
}
```

- [ ] **步骤 5：实现 commands.rs（扫描部分）**

```rust
use crate::db::Db;
use crate::model::{Filter, ScanStats};
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
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> std::result::Result<(), String> {
    settings::validate_quarantine_dir(&settings.quarantine_dir, &settings.roots)
        .map_err(|e| e.to_string())?;
    settings::save(&state.app_data, &settings).map_err(|e| e.to_string())?;
    *state.settings.lock() = settings;
    Ok(())
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) {
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
        (roots, settings.threads, state.db.clone(), state.cancel.clone())
    };
    if roots.is_empty() {
        return Err("还没有添加要扫描的文件夹".into());
    }
    cancel.store(false, Ordering::SeqCst);

    tauri::async_runtime::spawn_blocking(move || {
        let mut on_progress = |p: scanner::ScanProgress| {
            let _ = app.emit("scan://progress", p);
        };
        scanner::scan(&db, &roots, &ScanOptions { threads }, &mut on_progress)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
```

- [ ] **步骤 6：在 lib.rs 组装 Tauri Builder**

```rust
pub mod commands;
pub mod db;
pub mod error;
pub mod model;
pub mod nameparse;
pub mod scanner;
pub mod settings;

use commands::AppState;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data = app
                .path()
                .app_data_dir()
                .expect("app data dir");
            std::fs::create_dir_all(&app_data).ok();
            let db = db::Db::open(&app_data.join("picsieve.db")).expect("open db");
            db.migrate().expect("migrate");
            let s = settings::load(&app_data);
            app.manage(AppState {
                db: Arc::new(db),
                app_data,
                settings: parking_lot::Mutex::new(s),
                cancel: Arc::new(AtomicBool::new(false)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::start_scan,
            commands::cancel_scan,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **步骤 7：编译并运行测试**

```bash
cd /d/code/PicSieve/src-tauri
cargo test settings 2>&1 | tail -20
cargo build 2>&1 | tail -20
```

预期：2 个测试 PASS，`cargo build` 成功。

- [ ] **步骤 8：格式化、静态检查、提交**

```bash
cd /d/code/PicSieve/src-tauri
cargo fmt && cargo clippy --all-targets -- -D warnings
cd /d/code/PicSieve
git add -A
git commit -m "feat(commands): 设置读写、隔离区校验与扫描命令"
git push origin main
```

---

## 任务 6：前端骨架与扫描界面

**文件：**
- 创建：`src/types.ts`、`src/api.ts`、`src/components/ScanProgress.vue`
- 修改：`src/App.vue`、`src/main.ts`、`src/styles/main.css`
- 修改：`src-tauri/capabilities/default.json`

- [ ] **步骤 1：声明 Tauri 2 权限**

`src-tauri/capabilities/default.json`：

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "图筛主窗口所需权限",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:event:default",
    "dialog:allow-open",
    "dialog:allow-message"
  ]
}
```

- [ ] **步骤 2：编写 types.ts**

```ts
export interface FileRecord {
  id: number
  path: string
  root: string
  size: number
  mtime: number
  ext: string | null
  format: string | null
  width: number | null
  height: number | null
  shortSide: number | null
  pid: number | null
  artist: string | null
  contentHash: string | null
  phash: string | null
  grayScore: number | null
  decodeError: string | null
  status: string
}

export interface Filter {
  minShortSide: number | null
  maxShortSide: number | null
  minSize: number | null
  maxSize: number | null
  exts: string[]
  onlyGray: boolean
  onlyDuplicated: boolean
  onlyDecodeError: boolean
  search: string | null
  sort: string
  limit: number
  offset: number
}

export interface ScanProgress {
  seen: number
  totalHint: number
  current: string
}

export interface ScanStats {
  seen: number
  inserted: number
  updated: number
  skipped: number
  failed: number
}

export interface Settings {
  threads: number
  similarThreshold: number
  grayThreshold: number
  thumbMaxEdge: number
  thumbCacheLimitMb: number
  quarantineDir: string
  roots: string[]
}
```

- [ ] **步骤 3：编写 api.ts**

```ts
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import type { ScanProgress, ScanStats, Settings } from './types'

export const getSettings = () => invoke<Settings>('get_settings')
export const saveSettings = (settings: Settings) => invoke<void>('save_settings', { settings })
export const startScan = () => invoke<ScanStats>('start_scan')
export const cancelScan = () => invoke<void>('cancel_scan')

export function onScanProgress(cb: (p: ScanProgress) => void): Promise<UnlistenFn> {
  return listen<ScanProgress>('scan://progress', (e) => cb(e.payload))
}

export async function pickFolder(): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false, title: '选择要扫描的文件夹' })
  return typeof picked === 'string' ? picked : null
}
```

- [ ] **步骤 4：编写 ScanProgress.vue**

```vue
<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { cancelScan, onScanProgress, saveSettings, startScan, getSettings, pickFolder } from '../api'
import type { ScanStats, Settings } from '../types'
import type { UnlistenFn } from '@tauri-apps/api/event'

const settings = ref<Settings | null>(null)
const running = ref(false)
const seen = ref(0)
const totalHint = ref(0)
const current = ref('')
const stats = ref<ScanStats | null>(null)
const error = ref('')

const percent = computed(() =>
  totalHint.value > 0 ? Math.min(100, Math.round((seen.value / totalHint.value) * 100)) : 0,
)

let unlisten: UnlistenFn | null = null

onMounted(async () => {
  settings.value = await getSettings()
  unlisten = await onScanProgress((p) => {
    seen.value = p.seen
    totalHint.value = p.totalHint
    current.value = p.current
  })
})

onUnmounted(() => unlisten?.())

async function addRoot() {
  if (!settings.value) return
  const dir = await pickFolder()
  if (!dir || settings.value.roots.includes(dir)) return
  settings.value.roots = [...settings.value.roots, dir]
  await saveSettings(settings.value)
}

async function removeRoot(dir: string) {
  if (!settings.value) return
  settings.value.roots = settings.value.roots.filter((r) => r !== dir)
  await saveSettings(settings.value)
}

async function run() {
  error.value = ''
  stats.value = null
  seen.value = 0
  totalHint.value = 0
  running.value = true
  try {
    stats.value = await startScan()
  } catch (e) {
    error.value = String(e)
  } finally {
    running.value = false
  }
}
</script>

<template>
  <section class="panel">
    <h2>扫描文件夹</h2>

    <ul class="roots">
      <li v-for="r in settings?.roots ?? []" :key="r">
        <span class="path">{{ r }}</span>
        <button :disabled="running" @click="removeRoot(r)">移除</button>
      </li>
      <li v-if="!settings?.roots?.length" class="empty">还没有添加文件夹</li>
    </ul>

    <div class="row">
      <button :disabled="running" @click="addRoot">添加文件夹…</button>
      <button class="primary" :disabled="running || !settings?.roots?.length" @click="run">开始扫描</button>
      <button v-if="running" @click="cancelScan">取消</button>
    </div>

    <div v-if="running || stats" class="progress">
      <div class="bar"><div class="fill" :style="{ width: percent + '%' }" /></div>
      <p>{{ seen }} / {{ totalHint || '?' }}（{{ percent }}%）</p>
      <p class="path">{{ current }}</p>
    </div>

    <p v-if="stats" class="summary">
      新增 {{ stats.inserted }} · 更新 {{ stats.updated }} · 跳过 {{ stats.skipped }} · 失败 {{ stats.failed }}
    </p>
    <p v-if="error" class="error">{{ error }}</p>
  </section>
</template>

<style scoped>
.panel { padding: 24px; max-width: 860px; }
.roots { list-style: none; padding: 0; }
.roots li { display: flex; align-items: center; gap: 12px; padding: 6px 0; border-bottom: 1px solid var(--line); }
.path { font-size: 12px; opacity: 0.75; word-break: break-all; }
.empty { opacity: 0.5; }
.row { display: flex; gap: 10px; margin: 16px 0; }
button { padding: 6px 14px; border-radius: 4px; border: 1px solid var(--line); background: transparent; color: inherit; cursor: pointer; }
button:disabled { opacity: 0.4; cursor: default; }
.primary { background: #4a90d9; border-color: #4a90d9; color: #fff; }
.bar { height: 8px; background: var(--line); border-radius: 4px; overflow: hidden; }
.fill { height: 100%; background: #4a90d9; transition: width 0.15s; }
.summary { font-weight: 600; }
.error { color: #e05c4b; }
</style>
```

- [ ] **步骤 5：改写 App.vue 与 main.css**

`App.vue`：

```vue
<script setup lang="ts">
import ScanProgress from './components/ScanProgress.vue'
</script>

<template>
  <main>
    <h1>图筛 PicSieve</h1>
    <ScanProgress />
  </main>
</template>
```

`src/styles/main.css`：

```css
:root {
  --line: rgba(128, 128, 128, 0.28);
  --bg: #1b1d21;
  --fg: #e6e6e6;
}
* { box-sizing: border-box; }
html, body, #app { height: 100%; margin: 0; }
body { background: var(--bg); color: var(--fg); font: 14px/1.6 system-ui, 'Segoe UI', 'Microsoft YaHei', sans-serif; }
h1 { font-size: 18px; font-weight: 600; margin: 20px 24px 0; }
```

在 `main.ts` 里 import：`import './styles/main.css'`

- [ ] **步骤 6：端到端手工验证**

```bash
cd /d/code/PicSieve
pnpm tauri dev
```

在窗口里：

1. 点「添加文件夹…」，选一个**只含少量图片**的临时目录（不要直接选 `D:\色图`，那要跑一会）。
2. 点「开始扫描」，确认进度条走动、结束时显示「新增 N · 更新 0 · 跳过 0 · 失败 0」。
3. 再点一次「开始扫描」，确认这次显示「新增 0 · 跳过 N」（增量生效）。

预期：三条全部符合。若进度不更新，检查 `scan://progress` 事件名在前后端是否完全一致（大小写与斜杠）。

- [ ] **步骤 7：构建检查、提交、push**

```bash
cd /d/code/PicSieve
pnpm build
git add -A
git commit -m "feat(ui): 扫描界面与进度显示，打通前后端"
git push origin main
```

---

## 任务 7：内容指纹（大小预筛 + BLAKE3）

**文件：**
- 创建：`src-tauri/src/hashing.rs`
- 修改：`src-tauri/src/lib.rs`

**为什么要预筛：** 大小不同的文件绝不可能逐字节相同。规格第 2 节实测本批有 61,432 种尺寸组合，绝大多数文件大小唯一，因此大部分文件根本不需要读取内容。

- [ ] **步骤 1：添加依赖**

```bash
cd /d/code/PicSieve/src-tauri
cargo add blake3
```

- [ ] **步骤 2：编写失败的测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::model::FileRecord;

    fn rec(path: &str, size: i64) -> FileRecord {
        FileRecord { path: path.into(), root: "r".into(), size, mtime: 1, ..Default::default() }
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
```

- [ ] **步骤 3：运行测试验证失败**

```bash
cd /d/code/PicSieve/src-tauri
cargo test hashing 2>&1 | tail -20
```

预期：编译失败，`hash_file` / `candidate_groups` / `fingerprint_content` 未定义。

- [ ] **步骤 4：实现 hashing.rs**

```rust
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
    let mut stats = HashStats { groups: groups.len() as u64, ..Default::default() };

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
```

在 db.rs 补三个方法：

```rust
    pub fn query_column<T: rusqlite::types::FromSql>(&self, sql: &str, p: impl rusqlite::Params) -> Result<Vec<T>> {
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
        conn.execute("UPDATE files SET content_hash = ?1 WHERE id = ?2", params![hex, id])?;
        Ok(())
    }
```

- [ ] **步骤 5：运行测试验证通过**

```bash
cd /d/code/PicSieve/src-tauri
cargo test hashing 2>&1 | tail -25
```

预期：4 个测试 PASS。

- [ ] **步骤 6：格式化、静态检查、提交**

```bash
cd /d/code/PicSieve/src-tauri
cargo fmt && cargo clippy --all-targets -- -D warnings
cd /d/code/PicSieve
git add -A
git commit -m "feat(hashing): 大小预筛 + BLAKE3 内容指纹"
git push origin main
```

---

## 任务 8：感知哈希 pHash

**文件：**
- 创建：`src-tauri/src/phash.rs`
- 修改：`src-tauri/src/lib.rs`

- [ ] **步骤 1：编写失败的测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    /// 构造一张有渐变和色块的图，作为「特征明显的原图」
    fn busy_image(w: u32, h: u32, seed: u8) -> RgbImage {
        let mut img = RgbImage::new(w, h);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let r = ((x * 255 / w.max(1)) as u8).wrapping_add(seed);
            let g = (y * 255 / h.max(1)) as u8;
            let b = if (x / 8 + y / 8) % 2 == 0 { 40 } else { 200 };
            *p = Rgb([r, g, b]);
        }
        img
    }

    #[test]
    fn same_image_has_zero_distance() {
        let img = busy_image(300, 400, 0);
        let a = phash_u64(&image::DynamicImage::ImageRgb8(img.clone()));
        let b = phash_u64(&image::DynamicImage::ImageRgb8(img));
        assert_eq!(hamming(a, b), 0);
    }

    #[test]
    fn scaled_copy_stays_close() {
        let img = busy_image(300, 400, 0);
        let small = image::DynamicImage::ImageRgb8(img.clone()).resize_exact(120, 160, image::imageops::FilterType::Lanczos3);
        let a = phash_u64(&image::DynamicImage::ImageRgb8(img));
        let b = phash_u64(&small);
        assert!(hamming(a, b) <= 8, "缩放后汉明距离应仍很小，实际 {}", hamming(a, b));
    }

    #[test]
    fn different_images_are_far_apart() {
        let a = phash_u64(&image::DynamicImage::ImageRgb8(busy_image(300, 400, 0)));
        let b = phash_u64(&image::DynamicImage::ImageRgb8(busy_image(300, 400, 128)));
        assert!(hamming(a, b) > 4, "不同图不应过近，实际 {}", hamming(a, b));
    }

    #[test]
    fn hex_roundtrip() {
        let v = 0x0123_4567_89AB_CDEFu64;
        assert_eq!(from_hex(&to_hex(v)), Some(v));
        assert_eq!(from_hex("zzz"), None);
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cd /d/code/PicSieve/src-tauri
cargo test phash 2>&1 | tail -20
```

预期：编译失败，`phash_u64` 未定义。

- [ ] **步骤 3：实现 phash.rs**

```rust
use image::imageops::FilterType;
use image::{DynamicImage, GrayImage};
use std::f64::consts::PI;

const SIDE: usize = 32;
const KEEP: usize = 8;

/// 64 位感知哈希（pHash）。
///
/// 步骤：转灰度 → 缩到 32×32 → 二维 DCT-II → 取左上 8×8 → 与中位数比较成位。
/// 对缩放、重压缩稳定；对内容不同的图区分度足够。
pub fn phash_u64(img: &DynamicImage) -> u64 {
    let small: GrayImage = img.resize_exact(SIDE as u32, SIDE as u32, FilterType::Lanczos3).to_luma8();

    // 拉成 f64 矩阵
    let mut m = vec![vec![0f64; SIDE]; SIDE];
    for (x, y, p) in small.enumerate_pixels() {
        m[y as usize][x as usize] = p.0[0] as f64;
    }

    // 可分离 DCT-II：先对每行，再对每列
    let cos_table = cos_table();
    let rows: Vec<Vec<f64>> = m.iter().map(|row| dct_1d(row, &cos_table)).collect();
    let mut cols = vec![vec![0f64; SIDE]; SIDE];
    for x in 0..SIDE {
        let col: Vec<f64> = (0..SIDE).map(|y| rows[y][x]).collect();
        let out = dct_1d(&col, &cos_table);
        for y in 0..SIDE {
            cols[y][x] = out[y];
        }
    }

    // 取左上 8×8，跳过 DC 分量
    let mut vals = Vec::with_capacity(KEEP * KEEP - 1);
    for y in 0..KEEP {
        for x in 0..KEEP {
            if x == 0 && y == 0 {
                continue;
            }
            vals.push(cols[y][x]);
        }
    }
    let median = median_of(&mut vals);

    let mut bits = 0u64;
    let mut idx = 0;
    for y in 0..KEEP {
        for x in 0..KEEP {
            if x == 0 && y == 0 {
                continue;
            }
            if cols[y][x] > median {
                bits |= 1u64 << idx;
            }
            idx += 1;
        }
    }
    bits
}

fn cos_table() -> Vec<Vec<f64>> {
    (0..SIDE)
        .map(|k| {
            (0..SIDE)
                .map(|n| ((2.0 * n as f64 + 1.0) * k as f64 * PI / (2.0 * SIDE as f64)).cos())
                .collect()
        })
        .collect()
}

fn dct_1d(input: &[f64], cos_table: &[Vec<f64>]) -> Vec<f64> {
    (0..SIDE)
        .map(|k| {
            let s: f64 = (0..SIDE).map(|n| input[n] * cos_table[k][n]).sum();
            let scale = if k == 0 { (1.0 / SIDE as f64).sqrt() } else { (2.0 / SIDE as f64).sqrt() };
            s * scale
        })
        .collect()
}

fn median_of(vals: &mut Vec<f64>) -> f64 {
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = vals.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        vals[n / 2]
    } else {
        (vals[n / 2 - 1] + vals[n / 2]) / 2.0
    }
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

pub fn to_hex(v: u64) -> String {
    format!("{v:016x}")
}

pub fn from_hex(s: &str) -> Option<u64> {
    u64::from_str_radix(s, 16).ok()
}
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cd /d/code/PicSieve/src-tauri
cargo test phash 2>&1 | tail -25
```

预期：4 个测试 PASS。

若 `scaled_copy_stays_close` 失败，把断言里的距离上限放宽并**在提交信息里注明实测值**；不要直接把断言删掉。

- [ ] **步骤 5：格式化、静态检查、提交**

```bash
cd /d/code/PicSieve/src-tauri
cargo fmt && cargo clippy --all-targets -- -D warnings
cd /d/code/PicSieve
git add -A
git commit -m "feat(phash): 64 位感知哈希与汉明距离"
git push origin main
```

---

## 任务 9：灰度判定

**文件：**
- 创建：`src-tauri/src/gray.rs`
- 修改：`src-tauri/src/lib.rs`

- [ ] **步骤 1：编写失败的测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    fn solid(w: u32, h: u32, c: [u8; 3]) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(w, h, Rgb(c)))
    }

    #[test]
    fn pure_gray_scores_zero() {
        assert_eq!(gray_score(&solid(64, 64, [128, 128, 128])), 0.0);
    }

    #[test]
    fn black_and_white_gray_scores_zero() {
        let mut img = RgbImage::new(64, 64);
        for (x, _y, p) in img.enumerate_pixels_mut() {
            let v = if x < 32 { 0 } else { 255 };
            *p = Rgb([v, v, v]);
        }
        assert_eq!(gray_score(&DynamicImage::ImageRgb8(img)), 0.0);
    }

    #[test]
    fn saturated_color_scores_high() {
        assert!(gray_score(&solid(64, 64, [255, 0, 0])) > 100.0);
    }

    #[test]
    fn slightly_tinted_gray_scores_low() {
        assert!(gray_score(&solid(64, 64, [130, 128, 126])) <= 8.0);
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cd /d/code/PicSieve/src-tauri
cargo test gray 2>&1 | tail -20
```

预期：编译失败，`gray_score` 未定义。

- [ ] **步骤 3：实现 gray.rs**

```rust
use image::imageops::FilterType;
use image::DynamicImage;

const SIDE: usize = 64;

/// 灰度分数：先把图缩到 64×64 的 RGB，再逐像素取三通道两两差值中的最大值，
/// 最后取 95 分位。0 表示完全是黑白灰；数值越大越像彩色图。
pub fn gray_score(img: &DynamicImage) -> f64 {
    let small = img.resize_exact(SIDE as u32, SIDE as u32, FilterType::Lanczos3).to_rgb8();
    let mut diffs: Vec<f64> = Vec::with_capacity(SIDE * SIDE);
    for (_x, _y, p) in small.enumerate_pixels() {
        let r = p.0[0] as i32;
        let g = p.0[1] as i32;
        let b = p.0[2] as i32;
        let d = (r - g).abs().max((g - b).abs()).max((r - b).abs());
        diffs.push(d as f64);
    }
    percentile(&mut diffs, 95.0)
}

fn percentile(vals: &mut Vec<f64>, p: f64) -> f64 {
    if vals.is_empty() {
        return 0.0;
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((p / 100.0) * (vals.len() as f64 - 1.0)).round() as usize;
    vals[idx.min(vals.len() - 1)]
}
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cd /d/code/PicSieve/src-tauri
cargo test gray 2>&1 | tail -25
```

预期：4 个测试 PASS。

- [ ] **步骤 5：格式化、静态检查、提交**

```bash
cd /d/code/PicSieve/src-tauri
cargo fmt && cargo clippy --all-targets -- -D warnings
cd /d/code/PicSieve
git add -A
git commit -m "feat(gray): 灰度分数判定"
git push origin main
```

---

## 任务 10：指纹调度（并行 + 断点续算）

**文件：**
- 创建：`src-tauri/src/fingerprint.rs`
- 修改：`src-tauri/src/db.rs`、`src-tauri/src/lib.rs`、`src-tauri/src/commands.rs`

- [ ] **步骤 1：编写失败的测试**

```rust
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

        let stats = fingerprint_visual(&db, 2, &mut |_| {}).unwrap();
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

        let first = fingerprint_visual(&db, 1, &mut |_| {}).unwrap();
        let second = fingerprint_visual(&db, 1, &mut |_| {}).unwrap();
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
        assert!(fingerprint_visual(&db, 1, &mut |_| {}).is_ok());
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cd /d/code/PicSieve/src-tauri
cargo test fingerprint 2>&1 | tail -20
```

预期：编译失败，`fingerprint_visual` 未定义。

- [ ] **步骤 3：实现 fingerprint.rs**

```rust
use crate::db::{now_secs, Db};
use crate::error::{AppError, Result};
use crate::gray::gray_score;
use crate::model::FileRecord;
use crate::phash::{phash_u64, to_hex};
use rayon::prelude::*;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualStats {
    pub done: u64,
    pub skipped: u64,
    pub failed: u64,
}

/// 第二遍指纹：解码缩略图，算 pHash 与灰度分数。
///
/// 只处理 `phash IS NULL AND decode_error IS NULL` 的文件，因此天然支持断点续算。
/// GIF 由 `image` 解码器默认取第一帧。
pub fn fingerprint_visual(
    db: &Db,
    threads: usize,
    progress: &mut dyn FnMut(VisualStats),
) -> Result<VisualStats> {
    let pending = db.files_needing_visual()?;
    let mut stats = VisualStats::default();

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .build()
        .map_err(|e| AppError::Other(format!("线程池创建失败: {e}")))?;

    let batch = 512;
    for chunk in pending.chunks(batch) {
        let results: Vec<(i64, Result<(String, f64)>)> = pool.install(|| {
            chunk
                .par_iter()
                .map(|r: &FileRecord| (r.id, decode_and_hash(Path::new(&r.path))))
                .collect()
        });
        for (id, res) in results {
            match res {
                Ok((hex, gray)) => {
                    db.set_visual(id, &hex, gray, now_secs())?;
                    stats.done += 1;
                }
                Err(e) => {
                    db.set_decode_error(id, &e.to_string())?;
                    stats.failed += 1;
                }
            }
        }
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
        .map_err(|e| AppError::Image { path: path.to_path_buf(), source: e })?;
    Ok((to_hex(phash_u64(&img)), gray_score(&img)))
}
```

在 db.rs 补：

```rust
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
        conn.execute("UPDATE files SET decode_error=?1 WHERE id=?2", params![msg, id])?;
        Ok(())
    }

    pub fn count_visual_done(&self) -> Result<u64> {
        let conn = self.conn.lock();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM files WHERE phash IS NOT NULL", [], |r| r.get(0))?;
        Ok(n as u64)
    }
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cd /d/code/PicSieve/src-tauri
cargo test fingerprint 2>&1 | tail -25
```

预期：3 个测试 PASS。

- [ ] **步骤 5：接上 Tauri 命令**

在 `commands.rs` 增加：

```rust
#[tauri::command]
pub async fn start_fingerprint(app: AppHandle) -> std::result::Result<serde_json::Value, String> {
    // 与 start_scan 同理：指纹计算是阻塞重活，必须丢进线程池，否则界面假死。
    let (threads, db) = {
        let state = app.state::<AppState>();
        (state.settings.lock().threads, state.db.clone())
    };

    tauri::async_runtime::spawn_blocking(move || {
        let mut cb = |s: crate::hashing::HashStats| {
            let _ = app.emit("fingerprint://progress", serde_json::json!({ "phase": "content", "stats": s }));
        };
        let content = crate::hashing::fingerprint_content(&db, threads, &mut cb).map_err(|e| e.to_string())?;

        let mut cb2 = |s: crate::fingerprint::VisualStats| {
            let _ = app.emit("fingerprint://progress", serde_json::json!({ "phase": "visual", "stats": s }));
        };
        let visual = crate::fingerprint::fingerprint_visual(&db, threads, &mut cb2).map_err(|e| e.to_string())?;

        Ok::<_, String>(serde_json::json!({ "content": content, "visual": visual }))
    })
    .await
    .map_err(|e| e.to_string())?
}
```

并在 `invoke_handler` 里注册 `commands::start_fingerprint`。

- [ ] **步骤 6：编译、格式化、提交**

```bash
cd /d/code/PicSieve/src-tauri
cargo test 2>&1 | tail -10
cargo fmt && cargo clippy --all-targets -- -D warnings
cd /d/code/PicSieve
git add -A
git commit -m "feat(fingerprint): 并行视觉指纹调度与断点续算"
git push origin main
```

---

## 任务 11：完全重复分组与保留规则

**文件：**
- 创建：`src-tauri/src/grouper.rs`
- 修改：`src-tauri/src/db.rs`、`src-tauri/src/lib.rs`

- [ ] **步骤 1：编写失败的测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::model::FileRecord;

    fn rec(path: &str, hash: &str, pid: Option<i64>, mtime: i64) -> FileRecord {
        FileRecord {
            path: path.into(),
            root: "r".into(),
            size: 100,
            mtime,
            content_hash: Some(hash.into()),
            pid,
            ..Default::default()
        }
    }

    #[test]
    fn groups_files_with_identical_hash() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        db.upsert_file(&rec("a.jpg", "h1", None, 10)).unwrap();
        db.upsert_file(&rec("b.jpg", "h1", None, 20)).unwrap();
        db.upsert_file(&rec("c.jpg", "h2", None, 30)).unwrap();

        let groups = group_exact(&db).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), 2);
    }

    #[test]
    fn keeper_prefers_file_with_work_id() {
        // 规则 1：文件名含作品 ID 的优先保留
        let candidates = vec![
            rec("thumb_x.png", "h", None, 1),
            rec("12345678_p0.png", "h", Some(12345678), 2),
        ];
        let keep = choose_keeper(&candidates);
        assert_eq!(keep.path, "12345678_p0.png");
    }

    #[test]
    fn keeper_prefers_shorter_path_when_pid_ties() {
        // 规则 2：其次保留路径更短的
        let candidates = vec![
            rec("sub/dir/deep/aaa.jpg", "h", Some(1), 1),
            rec("aaa.jpg", "h", Some(1), 2),
        ];
        assert_eq!(choose_keeper(&candidates).path, "aaa.jpg");
    }

    #[test]
    fn keeper_prefers_earlier_mtime_when_all_tie() {
        // 规则 3：再其次保留修改时间更早的
        let candidates = vec![rec("late.jpg", "h", None, 99), rec("early.jpg", "h", None, 5)];
        assert_eq!(choose_keeper(&candidates).path, "early.jpg");
    }

    #[test]
    fn persists_groups_and_members() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        db.upsert_file(&rec("a.jpg", "h1", None, 10)).unwrap();
        db.upsert_file(&rec("b.jpg", "h1", None, 20)).unwrap();
        let groups = group_exact(&db).unwrap();
        persist_exact(&db, &groups).unwrap();
        let count: i64 = db.query_column("SELECT COUNT(*) FROM dup_members", rusqlite::params![]).unwrap()[0];
        assert_eq!(count, 2);
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cd /d/code/PicSieve/src-tauri
cargo test grouper 2>&1 | tail -20
```

预期：编译失败，`group_exact` 未定义。

- [ ] **步骤 3：实现 grouper.rs**

```rust
use crate::db::Db;
use crate::error::Result;
use crate::model::FileRecord;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactGroup {
    pub keep_id: i64,
    pub members: Vec<i64>,
}

/// 按 content_hash 分组，只返回成员数 > 1 的组。
pub fn group_exact(db: &Db) -> Result<Vec<ExactGroup>> {
    let hashes: Vec<String> = db.query_column(
        "SELECT content_hash FROM files
         WHERE status='normal' AND content_hash IS NOT NULL
         GROUP BY content_hash HAVING COUNT(*) > 1",
        rusqlite::params![],
    )?;

    let mut out = Vec::new();
    for h in hashes {
        let members = db.files_by_content_hash(&h)?;
        if members.len() < 2 {
            continue;
        }
        let keep = choose_keeper(&members);
        out.push(ExactGroup {
            keep_id: keep.id,
            members: members.iter().map(|m| m.id).collect(),
        });
    }
    Ok(out)
}

/// 保留规则（规格第 7.3）：含作品 ID 的优先 → 路径更短的优先 → 修改时间更早的优先。
/// 完全并列时取 id 最小者，保证结果稳定可复现。
pub fn choose_keeper(candidates: &[FileRecord]) -> FileRecord {
    candidates
        .iter()
        .min_by(|a, b| {
            let key = |r: &FileRecord| {
                (
                    if r.pid.is_some() { 0u8 } else { 1u8 },
                    r.path.len(),
                    r.mtime,
                    r.id,
                )
            };
            key(a).cmp(&key(b))
        })
        .expect("candidates 非空")
        .clone()
}

pub fn persist_exact(db: &Db, groups: &[ExactGroup]) -> Result<()> {
    db.clear_groups("exact")?;
    for g in groups {
        let gid = db.insert_group("exact", Some(g.keep_id))?;
        for id in &g.members {
            db.insert_member(gid, *id, 0)?;
        }
    }
    Ok(())
}
```

在 db.rs 补：

```rust
    pub fn files_by_content_hash(&self, h: &str) -> Result<Vec<FileRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT * FROM files WHERE content_hash=?1 AND status='normal'")?;
        let rows = stmt.query_map(params![h], row_to_record)?;
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
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cd /d/code/PicSieve/src-tauri
cargo test grouper 2>&1 | tail -25
```

预期：5 个测试 PASS。

- [ ] **步骤 5：格式化、静态检查、提交**

```bash
cd /d/code/PicSieve/src-tauri
cargo fmt && cargo clippy --all-targets -- -D warnings
cd /d/code/PicSieve
git add -A
git commit -m "feat(grouper): 完全重复分组与保留规则"
git push origin main
```

---

## 任务 12：相似分组（含同作品防误伤）

**文件：**
- 修改：`src-tauri/src/grouper.rs`、`src-tauri/src/db.rs`

**算法选择说明：** 13.7 万张两两比较约 9.4×10⁹ 对，看似很多，但每对只是一次 XOR + popcount，pHash 数组仅约 1 MB，可全部落在缓存里；用 rayon 并行后实测应在数秒到数十秒量级。这样实现简单且**不漏判**，优于引入 LSH 带来的召回损失。规模超过 50 万张时再考虑改 LSH，届时会更新规格。

- [ ] **步骤 1：编写失败的测试**

```rust
    // 追加到 grouper.rs 的 tests 模块
    #[test]
    fn page_number_is_extracted() {
        assert_eq!(page_of("12345678_p0.png"), Some(0));
        assert_eq!(page_of("12345678_p3.jpg"), Some(3));
        assert_eq!(page_of("#1 - 标题 - 画师 - [pid=12345678] -.png"), None);
    }

    #[test]
    fn same_work_different_pages_are_not_merged() {
        let a = VisualRec { id: 1, phash: 0xFFFF_FFFF_FFFF_FFFF, pid: Some(100), page: Some(0) };
        let b = VisualRec { id: 2, phash: 0xFFFF_FFFF_FFFF_FFFF, pid: Some(100), page: Some(1) };
        assert!(!should_merge(&a, &b, 8), "同作品不同页不能合并");
    }

    #[test]
    fn same_work_same_page_may_merge() {
        let a = VisualRec { id: 1, phash: 0xFFFF_FFFF_FFFF_FFFF, pid: Some(100), page: Some(0) };
        let b = VisualRec { id: 2, phash: 0xFFFF_FFFF_FFFF_FFFE, pid: Some(100), page: Some(0) };
        assert!(should_merge(&a, &b, 8));
    }

    #[test]
    fn unknown_page_info_still_merges() {
        let a = VisualRec { id: 1, phash: 0, pid: Some(100), page: None };
        let b = VisualRec { id: 2, phash: 1, pid: Some(100), page: None };
        assert!(should_merge(&a, &b, 8), "信息不足时不做防误伤判断，交给用户确认");
    }

    #[test]
    fn far_apart_phash_never_merges() {
        let a = VisualRec { id: 1, phash: 0x0000_0000_0000_0000, pid: None, page: None };
        let b = VisualRec { id: 2, phash: 0xFFFF_FFFF_FFFF_FFFF, pid: None, page: None };
        assert!(!should_merge(&a, &b, 8));
    }
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cd /d/code/PicSieve/src-tauri
cargo test grouper::tests::same_work 2>&1 | tail -20
```

预期：编译失败，`VisualRec` / `page_of` / `should_merge` 未定义。

- [ ] **步骤 3：实现相似分组**

```rust
#[derive(Debug, Clone)]
pub struct VisualRec {
    pub id: i64,
    pub phash: u64,
    pub pid: Option<i64>,
    pub page: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarGroup {
    pub keep_id: i64,
    pub members: Vec<(i64, u32)>,
}

/// 从文件名主名里提取 `_p数字` 页码片段。
pub fn page_of(stem: &str) -> Option<u32> {
    let lower = stem.to_ascii_lowercase();
    let idx = lower.rfind("_p")?;
    let rest = &lower[idx + 2..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// 是否应把两条视觉记录并入同一相似组。
///
/// 只要汉明距离不超阈值，且**不是「同作品的不同页」**，就合并。
/// 页码信息缺失时不做防误伤判断，交由用户人工确认。
pub fn should_merge(a: &VisualRec, b: &VisualRec, threshold: u32) -> bool {
    if crate::phash::hamming(a.phash, b.phash) > threshold {
        return false;
    }
    if let (Some(pa), Some(pb), Some(pp), Some(qp)) = (a.pid, b.pid, a.page, b.page) {
        if pa == pb && pp != qp {
            return false;
        }
    }
    true
}

/// 相似聚类：并查集 + 并行两两比较。
pub fn group_similar(db: &Db, threshold: u32, threads: usize) -> Result<Vec<SimilarGroup>> {
    use rayon::prelude::*;

    let recs = db.files_with_phash()?;
    let n = recs.len();
    if n < 2 {
        return Ok(Vec::new());
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .build()
        .map_err(|e| crate::error::AppError::Other(format!("线程池创建失败: {e}")))?;

    let phashes: Vec<u64> = recs.iter().map(|r| r.phash).collect();

    // 每个 i 只与 j > i 比较，收集需要合并的下标对
    let pairs: Vec<(usize, usize, u32)> = pool.install(|| {
        (0..n)
            .into_par_iter()
            .flat_map_iter(|i| {
                let mut local = Vec::new();
                for j in (i + 1)..n {
                    // should_merge 内部先比汉明距离，再应用「同作品不同页」规则
                    if should_merge(&recs[i], &recs[j], threshold) {
                        let d = crate::phash::hamming(phashes[i], phashes[j]);
                        local.push((i, j, d));
                    }
                }
                local
            })
            .collect()
    });

    let mut uf = UnionFind::new(n);
    for (i, j, _) in &pairs {
        uf.union(*i, *j);
    }

    let mut buckets: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for i in 0..n {
        buckets.entry(uf.find(i)).or_default().push(i);
    }

    let mut out = Vec::new();
    for members in buckets.into_values() {
        if members.len() < 2 {
            continue;
        }
        // 基准取「与其他成员平均距离最小」的那张（medoid）
        let base = members
            .iter()
            .copied()
            .min_by_key(|&i| {
                members
                    .iter()
                    .map(|&j| crate::phash::hamming(phashes[i], phashes[j]) as u64)
                    .sum::<u64>()
            })
            .expect("非空");
        let mut list: Vec<(i64, u32)> = members
            .iter()
            .filter(|&&i| i != base)
            .map(|&i| (recs[i].id, crate::phash::hamming(phashes[base], phashes[i])))
            .collect();
        list.sort_by_key(|(_, d)| *d);
        out.push(SimilarGroup { keep_id: recs[base].id, members: list });
    }
    out.sort_by(|a, b| a.members.first().map(|m| m.1).cmp(&b.members.first().map(|m| m.1)));
    Ok(out)
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self { parent: (0..n).collect() }
    }
    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            let root = self.find(self.parent[x]);
            self.parent[x] = root;
        }
        self.parent[x]
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[rb] = ra;
        }
    }
}
```

在 db.rs 补：

```rust
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
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cd /d/code/PicSieve/src-tauri
cargo test grouper 2>&1 | tail -25
```

预期：10 个测试 PASS。

- [ ] **步骤 5：加一个真实规模的性能测试（标 `#[ignore]`）**

```rust
    #[test]
    #[ignore = "需要真实数据，手动跑"]
    fn similar_clustering_scales_to_137k() {
        // 手工构造 137,000 条随机 pHash，测量 group_similar 的纯计算耗时。
        // 运行：cargo test --release grouper::tests::similar_clustering_scales_to_137k -- --ignored --nocapture
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        let mut rng: u64 = 0x1234_5678_9ABC_DEF0;
        for i in 0..137_000i64 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            db.upsert_file(&FileRecord {
                path: format!("f{i}.png"),
                root: "r".into(),
                size: 1,
                mtime: 1,
                phash: Some(crate::phash::to_hex(rng)),
                ..Default::default()
            })
            .unwrap();
        }
        let t = std::time::Instant::now();
        let groups = group_similar(&db, 8, 16).unwrap();
        println!("13.7 万条聚类耗时 {:?}，得到 {} 组", t.elapsed(), groups.len());
        assert!(t.elapsed().as_secs() < 300, "聚类不应超过 5 分钟");
    }
```

- [ ] **步骤 6：格式化、静态检查、提交**

```bash
cd /d/code/PicSieve/src-tauri
cargo fmt && cargo clippy --all-targets -- -D warnings
cd /d/code/PicSieve
git add -A
git commit -m "feat(grouper): pHash 相似聚类与同作品多页防误伤"
git push origin main
```

---

## 任务 13：条件筛选查询

**文件：**
- 创建：`src-tauri/src/query.rs`
- 修改：`src-tauri/src/db.rs`、`src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`

- [ ] **步骤 1：编写失败的测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::model::{FileRecord, Filter, SortKey};

    fn seed(db: &Db) {
        let rows = [
            ("a.jpg", 1000i64, 800i64, 600i64, 20.0f64, Some("h1")),
            ("b.png", 5000, 1600, 1200, 200.0, Some("h1")),
            ("c.gif", 300, 320, 240, 5.0, None),
        ];
        for (p, size, w, h, gray, hash) in rows {
            db.upsert_file(&FileRecord {
                path: p.into(),
                root: "r".into(),
                size,
                mtime: 1,
                ext: Some(p.rsplit('.').next().unwrap().into()),
                width: Some(w),
                height: Some(h),
                short_side: Some(h.min(w)),
                gray_score: Some(gray),
                content_hash: hash.map(str::to_string),
                ..Default::default()
            })
            .unwrap();
        }
        db.upsert_file(&FileRecord {
            path: "d.jpg".into(),
            root: "r".into(),
            size: 900,
            mtime: 1,
            ext: Some("jpg".into()),
            status: "quarantined".into(),
            ..Default::default()
        })
        .unwrap();
    }

    #[test]
    fn quarantined_files_are_never_listed() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db);
        let all = query_files(&db, &Filter { limit: 100, ..Default::default() }).unwrap();
        assert_eq!(all.len(), 3, "已进隔离区的文件不该再出现在图库里");
    }

    #[test]
    fn filters_by_short_side_range() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db);
        let f = Filter { min_short_side: Some(500), limit: 100, ..Default::default() };
        let got = query_files(&db, &f).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "a.jpg");
    }

    #[test]
    fn filters_by_ext_and_search() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db);
        let f = Filter { exts: vec!["png".into()], limit: 100, ..Default::default() };
        assert_eq!(query_files(&db, &f).unwrap().len(), 1);

        let f = Filter { search: Some("c.gif".into()), limit: 100, ..Default::default() };
        assert_eq!(query_files(&db, &f).unwrap().len(), 1);
    }

    #[test]
    fn only_gray_uses_threshold() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db);
        let f = Filter { only_gray: true, limit: 100, ..Default::default() };
        let got = query_files_gray(&db, &f, 8.0).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "c.gif");
    }

    #[test]
    fn only_duplicated_uses_hash_groups() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db);
        let f = Filter { only_duplicated: true, limit: 100, ..Default::default() };
        let got = query_files(&db, &f).unwrap();
        assert_eq!(got.len(), 2, "h1 有两个成员");
    }

    #[test]
    fn count_matches_query() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db);
        let f = Filter { limit: 100, ..Default::default() };
        assert_eq!(count_files(&db, &f).unwrap(), query_files(&db, &f).unwrap().len() as i64);
    }

    #[test]
    fn sorts_by_size_desc_by_default() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db);
        let got = query_files(&db, &Filter { limit: 100, sort: SortKey::SizeDesc, ..Default::default() }).unwrap();
        assert_eq!(got[0].path, "b.png");
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cd /d/code/PicSieve/src-tauri
cargo test query 2>&1 | tail -20
```

预期：编译失败，`query_files` 未定义。

- [ ] **步骤 3：实现 query.rs**

```rust
use crate::db::Db;
use crate::error::Result;
use crate::model::{FileRecord, Filter};
use rusqlite::types::Value;

/// 把 Filter 翻译成 WHERE 子句与参数。`only_gray` 需要外部阈值，故单独处理。
pub fn build_where(f: &Filter, gray_threshold: f64) -> (String, Vec<Value>) {
    let mut conds: Vec<String> = vec!["status = 'normal'".into()];
    let mut args: Vec<Value> = Vec::new();

    if let Some(v) = f.min_short_side {
        conds.push("short_side >= ?".into());
        args.push(Value::Integer(v));
    }
    if let Some(v) = f.max_short_side {
        conds.push("short_side <= ?".into());
        args.push(Value::Integer(v));
    }
    if let Some(v) = f.min_size {
        conds.push("size >= ?".into());
        args.push(Value::Integer(v));
    }
    if let Some(v) = f.max_size {
        conds.push("size <= ?".into());
        args.push(Value::Integer(v));
    }
    if !f.exts.is_empty() {
        let holes = vec!["?"; f.exts.len()].join(",");
        conds.push(format!("ext IN ({holes})"));
        for e in &f.exts {
            args.push(Value::Text(e.to_ascii_lowercase()));
        }
    }
    if f.only_gray {
        conds.push("gray_score IS NOT NULL AND gray_score <= ?".into());
        args.push(Value::Real(gray_threshold));
    }
    if f.only_decode_error {
        conds.push("decode_error IS NOT NULL".into());
    }
    if f.only_duplicated {
        conds.push(
            "content_hash IS NOT NULL AND content_hash IN (
               SELECT content_hash FROM files
               WHERE status='normal' AND content_hash IS NOT NULL
               GROUP BY content_hash HAVING COUNT(*) > 1)".into(),
        );
    }
    if let Some(s) = f.search.as_ref().filter(|s| !s.trim().is_empty()) {
        conds.push("path LIKE ?".into());
        args.push(Value::Text(format!("%{}%", s.trim())));
    }
    (conds.join(" AND "), args)
}

pub fn query_files(db: &Db, f: &Filter) -> Result<Vec<FileRecord>> {
    query_files_gray(db, f, 8.0)
}

pub fn query_files_gray(db: &Db, f: &Filter, gray_threshold: f64) -> Result<Vec<FileRecord>> {
    let (where_sql, args) = build_where(f, gray_threshold);
    let sql = format!(
        "SELECT * FROM files WHERE {where_sql} ORDER BY {} LIMIT ? OFFSET ?",
        f.sort.order_clause()
    );
    let mut all = args;
    all.push(Value::Integer(if f.limit <= 0 { 200 } else { f.limit }));
    all.push(Value::Integer(f.offset.max(0)));
    db.query_records(&sql, all)
}

pub fn count_files(db: &Db, f: &Filter) -> Result<i64> {
    count_files_gray(db, f, 8.0)
}

pub fn count_files_gray(db: &Db, f: &Filter, gray_threshold: f64) -> Result<i64> {
    let (where_sql, args) = build_where(f, gray_threshold);
    db.query_count(&format!("SELECT COUNT(*) FROM files WHERE {where_sql}"), args)
}
```

在 db.rs 补：

```rust
    pub fn query_records(&self, sql: &str, args: Vec<rusqlite::types::Value>) -> Result<Vec<FileRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), row_to_record)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn query_count(&self, sql: &str, args: Vec<rusqlite::types::Value>) -> Result<i64> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(sql)?;
        Ok(stmt.query_row(rusqlite::params_from_iter(args), |r| r.get(0))?)
    }
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cd /d/code/PicSieve/src-tauri
cargo test query 2>&1 | tail -25
```

预期：7 个测试 PASS。

- [ ] **步骤 5：加 Tauri 命令**

```rust
#[tauri::command]
pub fn query_files_cmd(state: State<'_, AppState>, filter: Filter) -> std::result::Result<Vec<FileRecord>, String> {
    let gray = state.settings.lock().gray_threshold;
    crate::query::query_files_gray(&state.db, &filter, gray).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn count_files_cmd(state: State<'_, AppState>, filter: Filter) -> std::result::Result<i64, String> {
    let gray = state.settings.lock().gray_threshold;
    crate::query::count_files_gray(&state.db, &filter, gray).map_err(|e| e.to_string())
}
```

注册到 `invoke_handler`。

- [ ] **步骤 6：格式化、静态检查、提交**

```bash
cd /d/code/PicSieve/src-tauri
cargo fmt && cargo clippy --all-targets -- -D warnings
cd /d/code/PicSieve
git add -A
git commit -m "feat(query): 条件筛选、分页与计数"
git push origin main
```

---

## 任务 14：缩略图缓存

**文件：**
- 创建：`src-tauri/src/thumb.rs`
- 修改：`src-tauri/src/db.rs`、`src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`

**为什么必须做：** 13.7 万张缩略图不能每次现算。规格设定缓存上限 3 GB，按 `id_mtime.jpg` 命名，源文件变了自然换新文件。

- [ ] **步骤 1：编写失败的测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn make(path: &std::path::Path, w: u32, h: u32) {
        RgbImage::from_pixel(w, h, Rgb([10, 200, 30])).save(path).unwrap();
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
        assert_eq!(first_mtime, std::fs::metadata(&b).unwrap().modified().unwrap(), "应命中缓存，不重写");
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
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cd /d/code/PicSieve/src-tauri
cargo test thumb 2>&1 | tail -20
```

预期：编译失败，`make_thumb` 未定义。

- [ ] **步骤 3：实现 thumb.rs**

```rust
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
        .map_err(|e| AppError::Image { path: src.to_path_buf(), source: e })?;

    let thumb = img.resize(max_edge, max_edge, FilterType::Triangle);
    // 先写临时文件再改名，避免进程中途退出留下半截 JPEG
    let tmp = out.with_extension("tmp");
    thumb
        .to_rgb8()
        .save_with_format(&tmp, image::ImageFormat::Jpeg)
        .map_err(|e| AppError::Image { path: tmp.clone(), source: e })?;
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
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cd /d/code/PicSieve/src-tauri
cargo test thumb 2>&1 | tail -25
```

预期：3 个测试 PASS。

- [ ] **步骤 5：加 Tauri 命令（返回 JPEG 字节）**

```rust
#[tauri::command]
pub fn get_thumb(app: AppHandle, state: State<'_, AppState>, file_id: i64) -> std::result::Result<Vec<u8>, String> {
    let rec = state.db.get_file(file_id).map_err(|e| e.to_string())?
        .ok_or_else(|| "文件不存在".to_string())?;
    let max_edge = state.settings.lock().thumb_max_edge;
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("thumbs");
    let p = crate::thumb::make_thumb(std::path::Path::new(&rec.path), &cache, rec.id, rec.mtime, max_edge)
        .map_err(|e| e.to_string())?;
    std::fs::read(&p).map_err(|e| e.to_string())
}
```

需要在文件顶部 `use tauri::Manager;`（`path()` 来自 `Manager`）。

- [ ] **步骤 6：格式化、静态检查、提交**

```bash
cd /d/code/PicSieve/src-tauri
cargo fmt && cargo clippy --all-targets -- -D warnings
cd /d/code/PicSieve
git add -A
git commit -m "feat(thumb): 带失效策略的缩略图缓存"
git push origin main
```

---

## 任务 15：图库主界面（筛选栏 + 虚拟网格）

**文件：**
- 创建：`src/stores/library.ts`、`src/components/FilterPanel.vue`、`src/components/ThumbGrid.vue`、`src/components/ThumbCell.vue`、`src/components/ActionBar.vue`
- 修改：`src/api.ts`、`src/App.vue`、`src/types.ts`

- [ ] **步骤 1：api.ts 追加接口**

```ts
import type { FileRecord, Filter } from './types'

export const queryFiles = (filter: Filter) => invoke<FileRecord[]>('query_files_cmd', { filter })
export const countFiles = (filter: Filter) => invoke<number>('count_files_cmd', { filter })

/** 取缩略图并转成可直接放进 <img src> 的 Blob URL。调用方负责在不用时 revoke。 */
export async function fetchThumbUrl(fileId: number): Promise<string> {
  const bytes = await invoke<number[]>('get_thumb', { fileId })
  const blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' })
  return URL.createObjectURL(blob)
}
```

- [ ] **步骤 2：装 pinia 并编写 library.ts（状态集中管理）**

```bash
cd /d/code/PicSieve
pnpm add pinia
```

```ts
import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { countFiles, queryFiles } from '../api'
import type { FileRecord, Filter } from '../types'

function emptyFilter(): Filter {
  return {
    minShortSide: null, maxShortSide: null, minSize: null, maxSize: null,
    exts: ['jpg', 'png', 'gif'], onlyGray: false, onlyDuplicated: false,
    onlyDecodeError: false, search: null, sort: 'sizeDesc', limit: 300, offset: 0,
  }
}

export const useLibrary = defineStore('library', () => {
  const filter = ref<Filter>(emptyFilter())
  const files = ref<FileRecord[]>([])
  const total = ref(0)
  const selected = ref<Set<number>>(new Set())
  const loading = ref(false)
  const error = ref('')

  const selectedCount = computed(() => selected.value.size)
  const selectedBytes = computed(() =>
    files.value.filter((f) => selected.value.has(f.id)).reduce((s, f) => s + f.size, 0),
  )

  async function refresh() {
    loading.value = true
    error.value = ''
    try {
      const [list, n] = await Promise.all([queryFiles(filter.value), countFiles(filter.value)])
      files.value = list
      total.value = n
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  function toggle(id: number) {
    const next = new Set(selected.value)
    next.has(id) ? next.delete(id) : next.add(id)
    selected.value = next
  }

  function clearSelection() {
    selected.value = new Set()
  }

  function patchFilter(patch: Partial<Filter>) {
    filter.value = { ...filter.value, ...patch, offset: 0 }
    return refresh()
  }

  return { filter, files, total, selected, loading, error, selectedCount, selectedBytes, refresh, toggle, clearSelection, patchFilter }
})
```

`package.json` 需要加 `pinia`：`pnpm add pinia`

- [ ] **步骤 3：编写 FilterPanel.vue**

```vue
<script setup lang="ts">
import { computed } from 'vue'
import { useLibrary } from '../stores/library'

const store = useLibrary()
const f = computed(() => store.filter)

// 分辨率与体积用「每档一个像素」的整数滑块，避免浮点比较问题
const SIZE_STEPS = [0, 50, 100, 200, 500, 1024, 2048, 5120, 10240, 51200] // KB

function setMinShort(v: number) { store.patchFilter({ minShortSide: v || null }) }
function setMaxShort(v: number) { store.patchFilter({ maxShortSide: v || null }) }
function setMinSize(i: number) { store.patchFilter({ minSize: (SIZE_STEPS[i] || 0) * 1024 || null }) }
function setMaxSize(i: number) { store.patchFilter({ maxSize: (SIZE_STEPS[i] || 0) * 1024 || null }) }
function toggleExt(e: string) {
  const cur = new Set(f.value.exts)
  cur.has(e) ? cur.delete(e) : cur.add(e)
  store.patchFilter({ exts: [...cur] })
}
</script>

<template>
  <aside class="filters">
    <label class="label">分辨率（短边）</label>
    <input type="range" min="0" max="3000" step="50" :value="f.minShortSide ?? 0"
           @input="setMinShort(+($event.target as HTMLInputElement).value)" />
    <input type="range" min="0" max="3000" step="50" :value="f.maxShortSide ?? 3000"
           @input="setMaxShort(+($event.target as HTMLInputElement).value)" />
    <p class="hint">{{ f.minShortSide ?? 0 }} – {{ f.maxShortSide ?? '不限' }} px</p>

    <label class="label">文件体积</label>
    <input type="range" min="0" max="9" step="1" :value="1" @input="setMinSize(+($event.target as HTMLInputElement).value)" />
    <input type="range" min="0" max="9" step="1" :value="9" @input="setMaxSize(+($event.target as HTMLInputElement).value)" />
    <p class="hint">拖动后自动重新查询</p>

    <label class="label">格式</label>
    <div class="chips">
      <button v-for="e in ['jpg', 'png', 'gif', 'webp', 'bmp']" :key="e"
              :class="{ on: f.exts.includes(e) }" @click="toggleExt(e)">{{ e.toUpperCase() }}</button>
    </div>

    <label class="label">只看</label>
    <label class="check"><input type="checkbox" :checked="f.onlyGray" @change="store.patchFilter({ onlyGray: !f.onlyGray })" /> 灰阶图（黑白灰）</label>
    <label class="check"><input type="checkbox" :checked="f.onlyDuplicated" @change="store.patchFilter({ onlyDuplicated: !f.onlyDuplicated })" /> 有重复的</label>
    <label class="check"><input type="checkbox" :checked="f.onlyDecodeError" @change="store.patchFilter({ onlyDecodeError: !f.onlyDecodeError })" /> 读不出的</label>

    <button class="reset" @click="store.patchFilter({ minShortSide: null, maxShortSide: null, minSize: null, maxSize: null, exts: ['jpg', 'png', 'gif'], onlyGray: false, onlyDuplicated: false, onlyDecodeError: false, search: null })">重置条件</button>
    <p class="count">命中 {{ store.total }} 张</p>
  </aside>
</template>

<style scoped>
.filters { width: 208px; flex: none; padding: 12px; border-right: 1px solid var(--line); font-size: 12px; }
.label { display: block; margin: 14px 0 6px; opacity: 0.6; font-size: 11px; text-transform: uppercase; }
input[type='range'] { width: 100%; }
.hint { margin: 4px 0 0; opacity: 0.6; font-size: 11px; }
.chips { display: flex; gap: 6px; flex-wrap: wrap; }
.chips button { padding: 2px 8px; border: 1px solid var(--line); border-radius: 3px; background: transparent; color: inherit; cursor: pointer; }
.chips button.on { background: rgba(74, 144, 217, 0.28); }
.check { display: block; margin: 4px 0; }
.reset { margin-top: 14px; background: transparent; border: 1px solid var(--line); color: inherit; border-radius: 4px; padding: 4px 10px; cursor: pointer; }
.count { margin-top: 10px; font-weight: 600; }
</style>
```

- [ ] **步骤 4：编写 ThumbCell.vue（单格，含角标）**

```vue
<script setup lang="ts">
import { onUnmounted, ref, watch } from 'vue'
import { fetchThumbUrl } from '../api'
import type { FileRecord } from '../types'

const props = defineProps<{ file: FileRecord; selected: boolean; dupCount: number; grayThreshold: number }>()
const emit = defineEmits<{ (e: 'toggle', id: number): void }>()

const url = ref('')
const failed = ref(false)

watch(() => props.file.id, async () => {
  if (url.value) { URL.revokeObjectURL(url.value); url.value = '' }
  try {
    url.value = await fetchThumbUrl(props.file.id)
  } catch {
    failed.value = true
  }
}, { immediate: true })

onUnmounted(() => { if (url.value) URL.revokeObjectURL(url.value) })
</script>

<template>
  <div class="cell" :class="{ on: props.selected }" @click="emit('toggle', props.file.id)">
    <img v-if="url" :src="url" loading="lazy" alt="" />
    <div v-else class="ph">{{ failed ? '读不出' : '…' }}</div>
    <span v-if="props.dupCount > 1" class="badge left">×{{ props.dupCount }}</span>
    <span v-if="(props.file.grayScore ?? 999) <= props.grayThreshold" class="badge left low">灰</span>
    <span v-if="props.selected" class="badge right">✓</span>
  </div>
</template>

<style scoped>
.cell { position: relative; aspect-ratio: 1 / 1.32; background: rgba(128, 128, 128, 0.15); border-radius: 4px; overflow: hidden; cursor: pointer; }
.cell.on { outline: 2px solid #4a90d9; outline-offset: -2px; }
img { width: 100%; height: 100%; object-fit: cover; display: block; }
.ph { display: grid; place-items: center; height: 100%; font-size: 11px; opacity: 0.5; }
.badge { position: absolute; top: 3px; font-size: 10px; padding: 0 4px; border-radius: 3px; background: rgba(0, 0, 0, 0.6); color: #fff; }
.badge.left { left: 3px; }
.badge.left.low { top: 20px; }
.badge.right { right: 3px; background: #4a90d9; }
</style>
```

- [ ] **步骤 5：编写 ThumbGrid.vue（虚拟滚动）**

```vue
<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import ThumbCell from './ThumbCell.vue'
import { useLibrary } from '../stores/library'

const store = useLibrary()
const scroller = ref<HTMLElement | null>(null)
const scrollTop = ref(0)
const viewportH = ref(800)
const CELL_W = 148
const CELL_H = 200

const columns = ref(6)
const rowCount = computed(() => Math.ceil(store.files.length / columns.value))
const totalH = computed(() => rowCount.value * CELL_H)

const startRow = computed(() => Math.max(0, Math.floor(scrollTop.value / CELL_H) - 2))
const endRow = computed(() => Math.min(rowCount.value, Math.ceil((scrollTop.value + viewportH.value) / CELL_H) + 2))
const visible = computed(() => store.files.slice(startRow.value * columns.value, endRow.value * columns.value))

const dupCount = ref<Map<number, number>>(new Map())

// 文件列表一变就重算角标（函数声明会提升，这里可以先引用后定义）
watch(() => store.files, computeDupCounts, { immediate: true })

function onScroll() {
  const el = scroller.value
  if (!el) return
  scrollTop.value = el.scrollTop
  viewportH.value = el.clientHeight
}

function measure() {
  const el = scroller.value
  if (!el) return
  columns.value = Math.max(1, Math.floor(el.clientWidth / CELL_W))
  viewportH.value = el.clientHeight
}

onMounted(() => {
  measure()
  store.refresh()
  window.addEventListener('resize', measure)
})

/** 用 content_hash 统计每张图的同内容成员数，供 ×N 角标使用。 */
function computeDupCounts() {
  const byHash = new Map<string, number>()
  for (const f of store.files) {
    if (f.contentHash) byHash.set(f.contentHash, (byHash.get(f.contentHash) ?? 0) + 1)
  }
  const m = new Map<number, number>()
  for (const f of store.files) {
    if (f.contentHash) m.set(f.id, byHash.get(f.contentHash) ?? 1)
  }
  dupCount.value = m
}
</script>

<template>
  <div ref="scroller" class="grid-scroll" @scroll.passive="onScroll">
    <div class="grid-inner" :style="{ height: totalH + 'px', gridTemplateColumns: `repeat(${columns}, 1fr)` }">
      <div class="grid-offset" :style="{ transform: `translateY(${startRow * CELL_H}px)`, gridTemplateColumns: `repeat(${columns}, 1fr)` }">
        <ThumbCell
          v-for="f in visible"
          :key="f.id"
          :file="f"
          :selected="store.selected.has(f.id)"
          :dup-count="dupCount.get(f.id) ?? 1"
          :gray-threshold="8"
          @toggle="store.toggle"
        />
      </div>
    </div>
    <p v-if="!store.files.length && !store.loading" class="empty">当前条件没有命中任何图片</p>
  </div>
</template>

<style scoped>
.grid-scroll { flex: 1; overflow-y: auto; padding: 10px; position: relative; }
.grid-inner { position: relative; }
.grid-offset { display: grid; gap: 8px; position: absolute; top: 0; left: 0; right: 0; }
.empty { text-align: center; opacity: 0.5; margin-top: 60px; }
</style>
```

- [ ] **步骤 6：编写 ActionBar.vue 并接进 App.vue**

```vue
<script setup lang="ts">
import { useLibrary } from '../stores/library'
const store = useLibrary()
defineEmits<{ (e: 'move-to-quarantine'): void }>()
</script>

<template>
  <footer class="bar">
    <span>已选 <strong>{{ store.selectedCount }}</strong> 张（{{ (store.selectedBytes / 1048576).toFixed(1) }} MB）</span>
    <button v-if="store.selectedCount" class="link" @click="store.clearSelection()">取消选择</button>
    <span class="flex" />
    <button class="danger" :disabled="!store.selectedCount" @click="$emit('move-to-quarantine')">移到隔离区</button>
  </footer>
</template>

<style scoped>
.bar { display: flex; align-items: center; gap: 14px; padding: 8px 14px; border-top: 1px solid var(--line); }
.link { background: none; border: none; color: #4a90d9; cursor: pointer; }
.flex { flex: 1; }
.danger { background: #c0553f; color: #fff; border: none; border-radius: 4px; padding: 5px 16px; cursor: pointer; }
.danger:disabled { opacity: 0.4; cursor: default; }
</style>
```

`App.vue` 改为：

```vue
<script setup lang="ts">
import { ref } from 'vue'
import ActionBar from './components/ActionBar.vue'
import FilterPanel from './components/FilterPanel.vue'
import ThumbGrid from './components/ThumbGrid.vue'
import ScanProgress from './components/ScanProgress.vue'

const tab = ref<'library' | 'scan'>('library')
</script>

<template>
  <header class="top">
    <strong>图筛 PicSieve</strong>
    <nav>
      <button :class="{ on: tab === 'library' }" @click="tab = 'library'">图库</button>
      <button :class="{ on: tab === 'scan' }" @click="tab = 'scan'">扫描</button>
    </nav>
  </header>

  <ScanProgress v-if="tab === 'scan'" />
  <template v-else>
    <div class="body">
      <FilterPanel />
      <ThumbGrid />
    </div>
    <ActionBar />
  </template>
</template>

<style scoped>
.top { display: flex; align-items: center; gap: 18px; padding: 8px 14px; border-bottom: 1px solid var(--line); }
nav button { background: none; border: none; color: inherit; opacity: 0.6; cursor: pointer; padding: 4px 8px; }
nav button.on { opacity: 1; border-bottom: 2px solid #4a90d9; }
.body { display: flex; flex: 1; min-height: 0; }
</style>
```

`main.ts` 挂 pinia：

```ts
import { createPinia } from 'pinia'
createApp(App).use(createPinia()).mount('#app')
```

同时 `#app` 与 `body` 需为纵向 flex 且 `height: 100%`（在 `main.css` 里补 `#app { display: flex; flex-direction: column; }`）。

- [ ] **步骤 7：手工验证**

```bash
cd /d/code/PicSieve
pnpm tauri dev
```

1. 先到「扫描」页扫一个含几十张图的目录。
2. 回「图库」页，确认缩略图出现。
3. 拖动分辨率滑块，确认「命中 N 张」跟着变。
4. 勾「只看灰阶」，确认只剩灰阶图（角标带 `灰`）。
5. 滚动几千张，确认不卡顿（虚拟滚动生效）。
6. 选几张，确认底部计数与体积跟着变。

预期：6 条全通过。若滚动卡，检查是否只渲染了可见行（打开 DevTools 数 DOM 节点）。

- [ ] **步骤 8：构建检查、提交**

```bash
cd /d/code/PicSieve
pnpm build
git add -A
git commit -m "feat(ui): 图库主界面、筛选栏与虚拟滚动网格"
git push origin main
```

---

## 任务 16：重复组审阅界面

**文件：**
- 修改：`src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`
- 创建：`src/components/DupGroupView.vue`
- 修改：`src/api.ts`、`src/types.ts`、`src/App.vue`

- [ ] **步骤 1：后端命令**

先把两个「给界面看的组合结构」加到 `model.rs`。放 model 层而不是 commands 层，数据库层才能引用它们，避免反向依赖：

```rust
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupView {
    pub group_id: i64,
    pub kind: String,
    pub keep: FileRecord,
    pub members: Vec<FileRecord>,
    pub distances: Vec<i64>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineBatch {
    pub batch_id: String,
    pub count: i64,
    pub bytes: i64,
    pub moved_at: i64,
}
```

再在 `commands.rs` 增加：

```rust
#[tauri::command]
pub fn list_dup_groups(
    state: State<'_, AppState>,
    kind: String,
    offset: i64,
    limit: i64,
) -> std::result::Result<Vec<GroupView>, String> {
    state.db.list_groups(&kind, offset, limit).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_keeper(state: State<'_, AppState>, group_id: i64, file_id: i64) -> std::result::Result<(), String> {
    state.db.set_group_keeper(group_id, file_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rebuild_groups(state: State<'_, AppState>) -> std::result::Result<serde_json::Value, String> {
    let threshold = state.settings.lock().similar_threshold;
    let threads = state.settings.lock().threads;
    let db = &state.db;
    let exact = crate::grouper::group_exact(db).map_err(|e| e.to_string())?;
    crate::grouper::persist_exact(db, &exact).map_err(|e| e.to_string())?;
    let similar = crate::grouper::group_similar(db, threshold, threads).map_err(|e| e.to_string())?;
    crate::grouper::persist_similar(db, &similar, threshold).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "exact": exact.len(), "similar": similar.len() }))
}
```

在 db.rs 补 `list_groups` / `set_group_keeper`，在 grouper.rs 补 `persist_similar`：

```rust
pub fn persist_similar(db: &Db, groups: &[SimilarGroup], _threshold: u32) -> Result<()> {
    db.clear_groups("similar")?;
    for g in groups {
        let gid = db.insert_group("similar", Some(g.keep_id))?;
        db.insert_member(gid, g.keep_id, 0)?;
        for (id, d) in &g.members {
            db.insert_member(gid, *id, *d as i64)?;
        }
    }
    Ok(())
}
```

```rust
    pub fn list_groups(&self, kind: &str, offset: i64, limit: i64) -> Result<Vec<crate::model::GroupView>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, keep_file_id FROM dup_groups WHERE kind=?1 ORDER BY id LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(params![kind, limit.max(1), offset.max(0)], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (gid, keep_id) = row?;
            let keep_id = keep_id.ok_or_else(|| crate::error::AppError::Schema("分组缺少保留项".into()))?;
            let keep = self.get_file(keep_id)?.ok_or_else(|| crate::error::AppError::Schema("保留项已不存在".into()))?;
            let members = self.group_members(gid)?;
            let distances = self.group_distances(gid)?;
            out.push(crate::model::GroupView { group_id: gid, kind: kind.to_string(), keep, members, distances });
        }
        Ok(out)
    }

    pub fn set_group_keeper(&self, group_id: i64, file_id: i64) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("UPDATE dup_groups SET keep_file_id=?1 WHERE id=?2", params![file_id, group_id])?;
        Ok(())
    }
```

`group_members`（排除 keeper）与 `group_distances` 按 `dup_members` 查询实现。

- [ ] **步骤 2：DupGroupView.vue**

要点：

- 顶部显示「第 N 组 / 共 M 组」
- 并排展示 keeper（蓝框）+ 其余成员
- 键盘 `←` `→` 换组、空格把当前焦点成员设为 keeper
- 底部「这 N 张移入隔离区」

```vue
<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { listDupGroups, setKeeper } from '../api'
import type { GroupView } from '../types'

const groups = ref<GroupView[]>([])
const index = ref(0)
const kind = ref<'exact' | 'similar'>('exact')
const current = () => groups.value[index.value]

async function load() {
  groups.value = await listDupGroups(kind.value, 0, 200)
  index.value = 0
}

function next() { if (index.value < groups.value.length - 1) index.value++ }
function prev() { if (index.value > 0) index.value-- }

async function keepAs(fileId: number) {
  const g = current()
  if (!g) return
  await setKeeper(g.groupId, fileId)
  g.keep = g.members.find((m) => m.id === fileId) ?? g.keep
  g.members = g.members.filter((m) => m.id !== fileId)
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'ArrowRight') next()
  else if (e.key === 'ArrowLeft') prev()
}

onMounted(() => { load(); window.addEventListener('keydown', onKey) })
onUnmounted(() => window.removeEventListener('keydown', onKey))
</script>

<template>
  <section class="wrap">
    <header class="head">
      <strong v-if="groups.length">第 {{ index + 1 }} 组 / 共 {{ groups.length }} 组</strong>
      <span v-else>还没有重复组，先去「扫描」页跑一次指纹与分组</span>
      <span class="flex" />
      <select v-model="kind" @change="load">
        <option value="exact">一模一样</option>
        <option value="similar">看着像</option>
      </select>
    </header>

    <div v-if="current()" class="row">
      <article class="card keep">
        <img :src="thumbUrl(current()!.keep.id)" alt="" />
        <div class="meta">
          <strong>{{ current()!.keep.width }} × {{ current()!.keep.height }}</strong>
          <span>{{ (current()!.keep.size / 1048576).toFixed(2) }} MB</span>
          <code>{{ current()!.keep.path }}</code>
          <em>✓ 保留这张</em>
        </div>
      </article>

      <article v-for="m in current()!.members" :key="m.id" class="card">
        <img :src="thumbUrl(m.id)" alt="" />
        <div class="meta">
          <strong>{{ m.width }} × {{ m.height }}</strong>
          <span>{{ (m.size / 1048576).toFixed(2) }} MB</span>
          <code>{{ m.path }}</code>
          <button @click="keepAs(m.id)">改留这张</button>
        </div>
      </article>
    </div>

    <footer class="foot">
      <button @click="prev" :disabled="index === 0">← 上一组</button>
      <button @click="next" :disabled="index >= groups.length - 1">下一组 →</button>
      <span class="flex" />
      <button class="danger" :disabled="!current()">这 {{ current()?.members.length ?? 0 }} 张移入隔离区</button>
    </footer>
  </section>
</template>
```

模板里用到的 `thumbUrl(id)` 由 `api.ts` 提供，内部用 Map 缓存 Blob URL，避免同一张图反复取。在 `api.ts` 追加：

```ts
const thumbCache = new Map<number, string>()

/** 同步读缓存；没取过就返回空串（组件会显示占位）。 */
export function thumbUrl(id: number): string {
  return thumbCache.get(id) ?? ''
}

/** 取图并缓存，返回可用的 Blob URL。 */
export async function preloadThumb(id: number): Promise<string> {
  const hit = thumbCache.get(id)
  if (hit) return hit
  const url = await fetchThumbUrl(id)
  thumbCache.set(id, url)
  return url
}

export function releaseThumbs(): void {
  for (const url of thumbCache.values()) URL.revokeObjectURL(url)
  thumbCache.clear()
}
```

`DupGroupView.vue` 在 `onMounted` 与换组时，对当前组里的每张图调用 `preloadThumb`，把结果写进一个 `ref<Map<number, string>>`，模板里读它即可。

- [ ] **步骤 3：手工验证**

```bash
cd /d/code/PicSieve
pnpm tauri dev
```

1. 至少构造一组已知重复：把一个图片文件复制一份放到被扫描目录里，改名。
2. 「扫描」页扫描 → 跑指纹 → 点「重建分组」。
3. 「重复组」页应显示该组，两张并排。
4. 按 `→` `←` 换组，点「改留这张」应改变蓝框归属。

预期：全部符合。

- [ ] **步骤 4：构建检查、提交**

```bash
cd /d/code/PicSieve
pnpm build
git add -A
git commit -m "feat(ui): 重复组审阅界面与键盘操作"
git push origin main
```

---

## 任务 17：隔离区（后端）

**文件：**
- 创建：`src-tauri/src/quarantine.rs`
- 修改：`src-tauri/src/db.rs`、`src-tauri/src/lib.rs`、`src-tauri/src/commands.rs`

**这是整个软件最危险的部分，测试必须覆盖「移入 → 搬回 → 内容不变」。**

- [ ] **步骤 1：编写失败的测试**

```rust
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
        assert_eq!(std::fs::read(&original).unwrap(), b"pretend jpeg bytes", "内容必须一字不差");
        assert_eq!(db.get_file(id).unwrap().unwrap().status, "normal");
    }

    #[test]
    fn purge_deletes_and_writes_log() {
        let (_src, q, db, id) = fixture();
        move_in(&db, &[id], q.path(), "batch-1").unwrap();
        let report = purge(&db, "batch-1").unwrap();
        assert_eq!(report.purged, 1);
        assert!(db.quarantine_batch("batch-1").unwrap().is_empty());

        let logged: i64 = db.query_column("SELECT COUNT(*) FROM delete_log", rusqlite::params![]).unwrap()[0];
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
```

- [ ] **步骤 2：运行测试验证失败**

```bash
cd /d/code/PicSieve/src-tauri
cargo test quarantine 2>&1 | tail -20
```

预期：编译失败，`move_in` 未定义。

- [ ] **步骤 3：实现 quarantine.rs**

```rust
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
        return Err(AppError::Other(format!("跨盘复制校验失败，已保留原文件: {}", src.display())));
    }
    std::fs::remove_file(src).map_err(|e| AppError::io(src, e))?;
    Ok(())
}

fn same_volume(a: &Path, b: &Path) -> bool {
    volume_of(a) == volume_of(b)
}

fn volume_of(p: &Path) -> Option<String> {
    let abs = std::fs::canonicalize(p).ok().unwrap_or_else(|| p.to_path_buf());
    abs.components().next().map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase())
}

/// 隔离区里若已有同名文件，追加 `__1`、`__2`… 直到不冲突。
fn unique_dest(root: &Path, src: &Path) -> Result<PathBuf> {
    let name = src.file_name().ok_or_else(|| AppError::Other("无文件名".into()))?;
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
```

在 db.rs 补：

```rust
    pub fn record_quarantine(&self, file_id: i64, original: &str, moved: &str, batch: &str) -> Result<()> {
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
        conn.execute("UPDATE files SET status=?1 WHERE id=?2", params![status, id])?;
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
        conn.execute("UPDATE quarantine SET restored_at=?1 WHERE file_id=?2 AND restored_at IS NULL", params![at, file_id])?;
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
```

- [ ] **步骤 4：运行测试验证通过**

```bash
cd /d/code/PicSieve/src-tauri
cargo test quarantine 2>&1 | tail -25
```

预期：5 个测试 PASS。

- [ ] **步骤 5：加 Tauri 命令**

```rust
#[tauri::command]
pub fn move_to_quarantine(state: State<'_, AppState>, file_ids: Vec<i64>) -> std::result::Result<crate::quarantine::MoveReport, String> {
    let q = state.settings.lock().quarantine_dir.clone();
    let batch = uuid::Uuid::new_v4().to_string();
    crate::quarantine::move_in(&state.db, &file_ids, std::path::Path::new(&q), &batch).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_batch(state: State<'_, AppState>, batch: String) -> std::result::Result<crate::quarantine::MoveReport, String> {
    crate::quarantine::restore(&state.db, &batch).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn purge_batch(state: State<'_, AppState>, batch: String) -> std::result::Result<crate::quarantine::PurgeReport, String> {
    crate::quarantine::purge(&state.db, &batch).map_err(|e| e.to_string())
}
```

- [ ] **步骤 6：格式化、静态检查、提交**

```bash
cd /d/code/PicSieve/src-tauri
cargo fmt && cargo clippy --all-targets -- -D warnings
cd /d/code/PicSieve
git add -A
git commit -m "feat(quarantine): 移入、搬回、彻底清空与操作留痕"
git push origin main
```

---

## 任务 18：隔离区界面

**文件：**
- 创建：`src/components/QuarantineView.vue`
- 修改：`src/api.ts`、`src/App.vue`、`src/types.ts`

- [ ] **步骤 1：后端补一个列批次的命令**

```rust
#[tauri::command]
pub fn list_quarantine_batches(state: State<'_, AppState>) -> std::result::Result<Vec<crate::model::QuarantineBatch>, String> {
    state.db.quarantine_batches().map_err(|e| e.to_string())
}
```

`QuarantineBatch` 已在任务 16 定义于 `model.rs`。

db.rs：

```rust
    pub fn quarantine_batches(&self) -> Result<Vec<crate::model::QuarantineBatch>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT batch_id, COUNT(*), MAX(moved_at) FROM quarantine WHERE restored_at IS NULL GROUP BY batch_id ORDER BY MAX(moved_at) DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (batch_id, count, moved_at) = row?;
            let bytes: i64 = conn.query_row(
                "SELECT COALESCE(SUM(f.size),0) FROM quarantine q JOIN files f ON f.id=q.file_id
                 WHERE q.batch_id=?1 AND q.restored_at IS NULL",
                params![batch_id],
                |r| r.get(0),
            )?;
            out.push(crate::model::QuarantineBatch { batch_id, count, bytes, moved_at });
        }
        Ok(out)
    }
```

- [ ] **步骤 2：QuarantineView.vue**

要点：

- 按批次列出：批号（取前 8 位）、张数、体积、时间
- 每批三个操作：「搬回来」「彻底清空」
- 「彻底清空」必须二次确认，确认文案里写明张数与释放空间

```vue
<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { listQuarantineBatches, purgeBatch, restoreBatch } from '../api'
import type { QuarantineBatch } from '../types'

const batches = ref<QuarantineBatch[]>([])
const msg = ref('')

async function load() { batches.value = await listQuarantineBatches() }

async function doRestore(b: QuarantineBatch) {
  const r = await restoreBatch(b.batchId)
  msg.value = `已搬回 ${r.moved} 张${r.failed ? `，失败 ${r.failed} 张` : ''}`
  await load()
}

async function doPurge(b: QuarantineBatch) {
  const gb = (b.bytes / 1073741824).toFixed(2)
  const ok = window.confirm(
    `将永久删除 ${b.count} 个文件，释放约 ${gb} GB。\n\n此操作不可撤销。确定继续？`,
  )
  if (!ok) return
  const r = await purgeBatch(b.batchId)
  msg.value = `已永久删除 ${r.purged} 张，释放 ${(r.bytes / 1073741824).toFixed(2)} GB`
  await load()
}

onMounted(load)
</script>

<template>
  <section class="wrap">
    <h2>隔离区</h2>
    <p class="hint">这里的文件只是被搬过来了，随时可以搬回原位。只有点了「彻底清空」才真的删除。</p>
    <p v-if="msg" class="msg">{{ msg }}</p>

    <table>
      <thead><tr><th>批次</th><th>张数</th><th>体积</th><th>操作</th></tr></thead>
      <tbody>
        <tr v-for="b in batches" :key="b.batchId">
          <td><code>{{ b.batchId.slice(0, 8) }}</code></td>
          <td>{{ b.count }}</td>
          <td>{{ (b.bytes / 1048576).toFixed(1) }} MB</td>
          <td class="ops">
            <button @click="doRestore(b)">搬回来</button>
            <button class="danger" @click="doPurge(b)">彻底清空</button>
          </td>
        </tr>
        <tr v-if="!batches.length"><td colspan="4" class="empty">隔离区是空的</td></tr>
      </tbody>
    </table>
  </section>
</template>

<style scoped>
.wrap { padding: 20px 24px; }
.hint { opacity: 0.65; font-size: 13px; }
.msg { color: #4a90d9; }
table { width: 100%; border-collapse: collapse; margin-top: 14px; }
th, td { text-align: left; padding: 8px; border-bottom: 1px solid var(--line); }
.ops { display: flex; gap: 8px; }
button { border: 1px solid var(--line); background: transparent; color: inherit; border-radius: 4px; padding: 3px 10px; cursor: pointer; }
.danger { background: #c0553f; border-color: #c0553f; color: #fff; }
.empty { opacity: 0.5; text-align: center; }
</style>
```

- [ ] **步骤 3：手工验证（破坏性）**

```bash
cd /d/code/PicSieve
pnpm tauri dev
```

1. 在一个**临时测试目录**里放 3 张图，扫描入库。
2. 图库页选中 2 张 → 「移到隔离区」。
3. 到「隔离区」页，确认批次显示 2 张；回资源管理器确认原文件已不在原处。
4. 点「搬回来」，确认文件回到原位且**内容没坏**（能正常打开）。
5. 再移入一次，点「彻底清空」，确认文件真的没了。

预期：5 步全通过。**这一步不许跳过。**

- [ ] **步骤 4：构建检查、提交**

```bash
cd /d/code/PicSieve
pnpm build
git add -A
git commit -m "feat(ui): 隔离区批次管理与二次确认清空"
git push origin main
```

---

## 任务 19：设置界面

**文件：**
- 创建：`src/components/SettingsView.vue`
- 修改：`src/App.vue`

规格第 9.5 节要求所有阈值可调。后端 `get_settings` / `save_settings` 已在任务 5 完成，本任务只做界面。

- [ ] **步骤 1：编写 SettingsView.vue**

```vue
<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { getSettings, pickFolder, saveSettings } from '../api'
import type { Settings } from '../types'

const s = ref<Settings | null>(null)
const msg = ref('')
const err = ref('')

onMounted(async () => {
  s.value = await getSettings()
})

async function chooseQuarantine() {
  const dir = await pickFolder()
  if (dir && s.value) s.value.quarantineDir = dir
}

async function save() {
  if (!s.value) return
  err.value = ''
  msg.value = ''
  try {
    await saveSettings(s.value)
    msg.value = '已保存'
  } catch (e) {
    err.value = String(e)
  }
}
</script>

<template>
  <section v-if="s" class="wrap">
    <h2>设置</h2>

    <div class="row">
      <span class="k">线程数</span>
      <input type="number" min="1" max="64" v-model.number="s.threads" />
      <em>默认「逻辑核心数 − 2」，给你的电脑留出响应余量</em>
    </div>

    <div class="row">
      <span class="k">相似判定阈值</span>
      <input type="range" min="0" max="16" v-model.number="s.similarThreshold" />
      <b>{{ s.similarThreshold }}</b>
      <em>越小越严：越不容易误判，也越可能漏掉；越大越宽，误判会变多</em>
    </div>

    <div class="row">
      <span class="k">灰阶判定宽松度</span>
      <input type="range" min="0" max="30" v-model.number="s.grayThreshold" />
      <b>{{ s.grayThreshold }}</b>
      <em>越小只认纯黑白灰；越大越容易把偏灰的彩图也当成灰阶</em>
    </div>

    <div class="row">
      <span class="k">缩略图长边</span>
      <input type="number" min="80" max="640" v-model.number="s.thumbMaxEdge" />
      <em>像素。越大越清楚，缓存也越占地方</em>
    </div>

    <div class="row">
      <span class="k">缩略图缓存上限</span>
      <input type="number" min="256" max="20480" v-model.number="s.thumbCacheLimitMb" />
      <em>MB</em>
    </div>

    <div class="row">
      <span class="k">隔离区目录</span>
      <input class="wide" v-model="s.quarantineDir" />
      <button @click="chooseQuarantine">选择…</button>
      <em>必须放在扫描目录之外，否则下次扫描会把待删文件又捞回来</em>
    </div>

    <div class="actions">
      <button class="primary" @click="save">保存</button>
      <span v-if="msg" class="ok">{{ msg }}</span>
      <span v-if="err" class="error">{{ err }}</span>
    </div>
  </section>
</template>

<style scoped>
.wrap { padding: 20px 24px; max-width: 900px; }
.row { display: flex; align-items: center; gap: 12px; padding: 10px 0; border-bottom: 1px solid var(--line); flex-wrap: wrap; }
.k { width: 130px; flex: none; }
.row input[type='range'] { width: 240px; }
.row input[type='number'] { width: 90px; }
.wide { flex: 1; min-width: 260px; }
em { opacity: 0.6; font-size: 12px; flex-basis: 100%; }
input { background: rgba(128, 128, 128, 0.12); border: 1px solid var(--line); color: inherit; border-radius: 4px; padding: 4px 8px; }
button { border: 1px solid var(--line); background: transparent; color: inherit; border-radius: 4px; padding: 4px 12px; cursor: pointer; }
.primary { background: #4a90d9; border-color: #4a90d9; color: #fff; }
.actions { display: flex; align-items: center; gap: 14px; margin-top: 18px; }
.ok { color: #4a90d9; }
.error { color: #e05c4b; }
</style>
```

- [ ] **步骤 2：接进 App.vue 的标签栏**

`tab` 的联合类型加上 `'settings'`，导航里加一个「设置」按钮，并用 `v-if` 渲染 `<SettingsView />`。

- [ ] **步骤 3：手工验证**

```bash
cd /d/code/PicSieve
pnpm tauri dev
```

1. 把「相似判定阈值」从 8 改成 4，保存，关掉软件重开，确认仍是 4。
2. 把「隔离区目录」改成扫描目录里面的一个子目录，点保存，确认**报错**并给出人话提示（这是规格第 10.8 条要求的保护）。
3. 把隔离区目录改回合法路径，保存成功。

预期：3 条全通过。

- [ ] **步骤 4：构建检查、提交**

```bash
cd /d/code/PicSieve
pnpm build
git add -A
git commit -m "feat(ui): 设置界面，阈值与目录可调"
git push origin main
```

---

## 任务 20：打包与真机冒烟

**文件：**
- 修改：`src-tauri/tauri.conf.json`、`package.json`
- 创建：`README.md`

- [ ] **步骤 1：配置打包**

`src-tauri/tauri.conf.json` 的 `bundle` 段设为：

```json
{
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/icon.ico"],
    "windows": {
      "nsis": {
        "installMode": "currentUser",
        "languages": ["SimpChinese"]
      }
    }
  }
}
```

> `installMode` 用 `currentUser`，避免要求管理员权限。产品名与标识符保持 `PicSieve` / `com.landslide.picsieve`。

- [ ] **步骤 2：写 README.md**

内容至少包含：这是什么、给谁用、当前状态（MVP）、如何开发（`pnpm install` / `pnpm tauri dev`）、如何打包（`pnpm tauri build`）、安全说明（删除即移动到隔离区；扫描阶段只读）、以及指向 `docs/superpowers/specs/` 与 `docs/superpowers/plans/` 的链接。

- [ ] **步骤 3：打包**

```bash
cd /d/code/PicSieve
pnpm tauri build
```

预期：产物在 `src-tauri/target/release/bundle/nsis/`，同时 `src-tauri/target/release/picsieve.exe` 可直接运行。

- [ ] **步骤 4：真机冒烟清单**

在**打包产物**（不是 dev 模式）上逐条走：

- [ ] 安装到默认路径，能启动，窗口标题为「图筛 PicSieve」
- [ ] 添加扫描目录 `D:\色图\2017-2024 PIXIV daily`
- [ ] 跑扫描：耗时应在 2 分钟内；结束后总数应接近 86,270
- [ ] 跑指纹：记录实际耗时，与规格第 11 节的 ≤ 15 分钟目标比对
- [ ] 图库页滚动流畅，筛选滑块响应无明显延迟
- [ ] 「只看灰阶」命中数量与规格第 2 节的抽样比例（约 5%）大致同量级
- [ ] 在一组重复里改保留项，再移入隔离区，搬回，确认内容完好
- [ ] 关闭软件再打开，数据仍在（数据库持久化生效）

- [ ] **步骤 5：把实测数据回填规格**

把第 4 步记录到的真实耗时与命中数写回 `docs/superpowers/specs/2026-10-05-picsieve-design.md` 第 11 节（性能目标）旁边，注明「实测」二字。规格里的目标值只有被实测校准过才有意义。

- [ ] **步骤 6：提交并推送**

```bash
cd /d/code/PicSieve
git add -A
git commit -m "chore: 打包配置、README 与真机冒烟实测记录"
git push origin main
```

---

## 附：任务依赖关系

```
1 ── 2 ── 3 ── 4 ── 5 ── 6
                    │
              7 ── 8 ── 9 ── 10
                    │
             11 ── 12 ── 13
                    │
             14 ── 15 ── 16
                    │
             17 ── 18 ──┐
                        ├── 20
              5 ── 19 ───┘
```

- 任务 7–10 依赖任务 2（数据库）与 4（扫描出的记录）。
- 任务 12 依赖任务 8（pHash）。
- 任务 15 依赖任务 13（查询）与 14（缩略图）。
- 任务 16 依赖任务 11、12。
- 任务 18 依赖任务 17。
- 任务 19 依赖任务 5（设置读写已就绪）。
- 任务 20 依赖全部。

同一条链上的任务必须顺序执行；不同链（如 7–10 与 14）在各自前置完成后可并行。

---

## 附：本计划刻意不含的内容

这些属于规格第 14 节「明确不做的事」，实现时**不要**顺手加进来：

- 图片编辑、重命名、按画师重新整理目录
- 画面语义识别（是否漫画页、人脸、NSFW 分类）
- macOS / Linux 支持
- 云同步、多用户
- 超过 50 万张规模所需的 LSH 优化（当前算法在 13.7 万张规模下实测通过即可；届时另开规格修订）

