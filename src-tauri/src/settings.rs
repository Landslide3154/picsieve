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
        let n = std::thread::available_parallelism()
            .map(|v| v.get())
            .unwrap_or(4);
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
    if dir.trim().is_empty() {
        return Err(AppError::Other("隔离区目录不能为空".into()));
    }
    let q = normalize(dir);
    for r in roots {
        let rn = normalize(r);
        if rn.is_empty() {
            continue;
        }
        if q == rn || q.starts_with(&(rn.clone() + "\\")) {
            return Err(AppError::Other(format!(
                "隔离区不能放在扫描目录「{r}」里面，否则下次扫描会把它当成待处理文件"
            )));
        }
    }
    Ok(())
}

fn normalize(p: &str) -> String {
    p.replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let s = Settings::default();
        let n = std::thread::available_parallelism()
            .map(|v| v.get())
            .unwrap_or(4);
        assert_eq!(
            s.threads,
            n.saturating_sub(2).max(1),
            "默认线程数 = 逻辑核心数 - 2"
        );
        assert_eq!(s.similar_threshold, 8);
        assert_eq!(s.gray_threshold, 8.0);
        assert_eq!(s.thumb_cache_limit_mb, 3072);
    }

    #[test]
    fn quarantine_must_not_be_inside_scan_root() {
        let roots = vec![r"D:\色图\2017-2024 PIXIV daily".to_string()];
        assert!(validate_quarantine_dir(r"D:\色图\_待确认删除", &roots).is_ok());
        assert!(
            validate_quarantine_dir(r"D:\色图\2017-2024 PIXIV daily\_待确认删除", &roots).is_err()
        );
    }
}
