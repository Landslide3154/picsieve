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
               GROUP BY content_hash HAVING COUNT(*) > 1)"
                .into(),
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
    db.query_count(
        &format!("SELECT COUNT(*) FROM files WHERE {where_sql}"),
        args,
    )
}

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
        let all = query_files(
            &db,
            &Filter {
                limit: 100,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(all.len(), 3, "已进隔离区的文件不该再出现在图库里");
    }

    #[test]
    fn filters_by_short_side_range() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db);
        // 短边 >= 500 应同时命中 a.jpg(600) 与 b.png(1200)，c.gif(240) 排除
        let f = Filter {
            min_short_side: Some(500),
            limit: 100,
            ..Default::default()
        };
        let got = query_files(&db, &f).unwrap();
        let mut paths: Vec<&str> = got.iter().map(|r| r.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(paths, vec!["a.jpg", "b.png"]);

        // 抬高下界到 700，只剩 b.png
        let f = Filter {
            min_short_side: Some(700),
            limit: 100,
            ..Default::default()
        };
        let got = query_files(&db, &f).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "b.png");
    }

    #[test]
    fn filters_by_ext_and_search() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db);
        let f = Filter {
            exts: vec!["png".into()],
            limit: 100,
            ..Default::default()
        };
        assert_eq!(query_files(&db, &f).unwrap().len(), 1);

        let f = Filter {
            search: Some("c.gif".into()),
            limit: 100,
            ..Default::default()
        };
        assert_eq!(query_files(&db, &f).unwrap().len(), 1);
    }

    #[test]
    fn only_gray_uses_threshold() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db);
        let f = Filter {
            only_gray: true,
            limit: 100,
            ..Default::default()
        };
        let got = query_files_gray(&db, &f, 8.0).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "c.gif");
    }

    #[test]
    fn only_duplicated_uses_hash_groups() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db);
        let f = Filter {
            only_duplicated: true,
            limit: 100,
            ..Default::default()
        };
        let got = query_files(&db, &f).unwrap();
        assert_eq!(got.len(), 2, "h1 有两个成员");
    }

    #[test]
    fn count_matches_query() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db);
        let f = Filter {
            limit: 100,
            ..Default::default()
        };
        assert_eq!(
            count_files(&db, &f).unwrap(),
            query_files(&db, &f).unwrap().len() as i64
        );
    }

    #[test]
    fn sorts_by_size_desc_by_default() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        seed(&db);
        let got = query_files(
            &db,
            &Filter {
                limit: 100,
                sort: SortKey::SizeDesc,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(got[0].path, "b.png");
    }
}
