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
            let key = |r: &FileRecord| (u8::from(r.pid.is_none()), r.path.len(), r.mtime, r.id);
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

/// 「同作品的不同页」——这是唯一的防误伤规则。
///
/// 只有两边都识别出 pid、且 pid 相同、页码也都识别出来且不同，才算不同页。
/// 信息不足时返回 false（不阻止合并），交由用户人工确认。
fn pages_conflict(a: &VisualRec, b: &VisualRec) -> bool {
    if let (Some(pa), Some(pb), Some(pp), Some(qp)) = (a.pid, b.pid, a.page, b.page) {
        return pa == pb && pp != qp;
    }
    false
}

/// 是否应把两条视觉记录并入同一相似组。
///
/// 只要汉明距离不超阈值，且**不是「同作品的不同页」**，就合并。
/// 页码信息缺失时不做防误伤判断，交由用户人工确认。
pub fn should_merge(a: &VisualRec, b: &VisualRec, threshold: u32) -> bool {
    if crate::phash::hamming(a.phash, b.phash) > threshold {
        return false;
    }
    !pages_conflict(a, b)
}

/// 相似聚类：并查集 + 并行两两比较。
///
/// 13.7 万张两两比较约 9.4×10⁹ 对，每对只是一次 XOR + popcount，
/// pHash 数组约 1 MB 可全部落在缓存里，因此不做 LSH，保证不漏判。
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
                    let d = crate::phash::hamming(phashes[i], phashes[j]);
                    if d <= threshold && !pages_conflict(&recs[i], &recs[j]) {
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

    let mut buckets: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
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
        list.sort_by_key(|(id, d)| (*d, *id));
        out.push(SimilarGroup {
            keep_id: recs[base].id,
            members: list,
        });
    }
    // 输出顺序固定：先按最像的成员距离，再按保留项 id，保证结果可复现
    out.sort_by_key(|g| (g.members.first().map(|m| m.1), g.keep_id));
    Ok(out)
}

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

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
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
        // 规则 3：再其次保留修改时间更早的。
        // 两条路径必须**等长**，否则会先被规则 2（路径更短优先）决定。
        let candidates = vec![
            rec("late9.jpg", "h", None, 99),
            rec("early.jpg", "h", None, 5),
        ];
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
        let count: i64 = db
            .query_column("SELECT COUNT(*) FROM dup_members", rusqlite::params![])
            .unwrap()[0];
        assert_eq!(count, 2);
    }

    #[test]
    fn page_number_is_extracted() {
        assert_eq!(page_of("12345678_p0.png"), Some(0));
        assert_eq!(page_of("12345678_p3.jpg"), Some(3));
        assert_eq!(page_of("#1 - 标题 - 画师 - [pid=12345678] -.png"), None);
    }

    #[test]
    fn same_work_different_pages_are_not_merged() {
        let a = VisualRec {
            id: 1,
            phash: 0xFFFF_FFFF_FFFF_FFFF,
            pid: Some(100),
            page: Some(0),
        };
        let b = VisualRec {
            id: 2,
            phash: 0xFFFF_FFFF_FFFF_FFFF,
            pid: Some(100),
            page: Some(1),
        };
        assert!(!should_merge(&a, &b, 8), "同作品不同页不能合并");
    }

    #[test]
    fn same_work_same_page_may_merge() {
        let a = VisualRec {
            id: 1,
            phash: 0xFFFF_FFFF_FFFF_FFFF,
            pid: Some(100),
            page: Some(0),
        };
        let b = VisualRec {
            id: 2,
            phash: 0xFFFF_FFFF_FFFF_FFFE,
            pid: Some(100),
            page: Some(0),
        };
        assert!(should_merge(&a, &b, 8));
    }

    #[test]
    fn unknown_page_info_still_merges() {
        let a = VisualRec {
            id: 1,
            phash: 0,
            pid: Some(100),
            page: None,
        };
        let b = VisualRec {
            id: 2,
            phash: 1,
            pid: Some(100),
            page: None,
        };
        assert!(
            should_merge(&a, &b, 8),
            "信息不足时不做防误伤判断，交给用户确认"
        );
    }

    #[test]
    fn far_apart_phash_never_merges() {
        let a = VisualRec {
            id: 1,
            phash: 0x0000_0000_0000_0000,
            pid: None,
            page: None,
        };
        let b = VisualRec {
            id: 2,
            phash: 0xFFFF_FFFF_FFFF_FFFF,
            pid: None,
            page: None,
        };
        assert!(!should_merge(&a, &b, 8));
    }

    /// 手工构造 137,000 条随机 pHash，测量 group_similar 的纯计算耗时。
    /// 运行：cargo test --release grouper::tests::similar_clustering_scales_to_137k -- --ignored --nocapture
    #[test]
    #[ignore = "需要真实规模，手动跑"]
    fn similar_clustering_scales_to_137k() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        let mut rng: u64 = 0x1234_5678_9ABC_DEF0;
        for i in 0..137_000i64 {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
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
        println!(
            "13.7 万条聚类耗时 {:?}，得到 {} 组",
            t.elapsed(),
            groups.len()
        );
        assert!(t.elapsed().as_secs() < 300, "聚类不应超过 5 分钟");
    }
}
