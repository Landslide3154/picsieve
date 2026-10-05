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

/// 重复组的界面视图：保留项 + 其余成员 + 各成员到基准的距离。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupView {
    pub group_id: i64,
    pub kind: String,
    pub keep: FileRecord,
    pub members: Vec<FileRecord>,
    pub distances: Vec<i64>,
}

/// 隔离区里的一批文件。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineBatch {
    pub batch_id: String,
    pub count: i64,
    pub bytes: i64,
    pub moved_at: i64,
}
