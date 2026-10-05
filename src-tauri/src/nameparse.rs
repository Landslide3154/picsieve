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

#[cfg(test)]
mod tests {
    use super::*;

    // 全部取自用户真实文件名样本（规格第 2 节）
    #[test]
    fn parses_pid_with_brackets() {
        assert_eq!(
            parse_pid("白ウサギ - 赤倉@初画集＆個展 - [pid=115088821] -"),
            Some(115088821)
        );
    }

    #[test]
    fn parses_pid_uppercase_prefix() {
        assert_eq!(
            parse_pid("#1 - 愛言葉Ⅴ - おむたつ／omutatsu - PID=141311340 -"),
            Some(141311340)
        );
    }

    #[test]
    fn parses_pid_attached_to_title() {
        assert_eq!(
            parse_pid("【yae】　狼ト生キル - 成人式[pid=48119895]"),
            Some(48119895)
        );
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
        assert_eq!(
            parse_artist("#1 - ∞ - Rella - 133377800 -"),
            Some("Rella".to_string())
        );
    }

    #[test]
    fn returns_none_when_artist_unknown() {
        assert_eq!(parse_artist("【yae】　狼ト生キル - 成人式[pid=48119895]"), None);
        assert_eq!(parse_artist("-.jpg"), None);
    }

    #[test]
    fn does_not_treat_ranking_number_as_pid() {
        // "#1" 是排行榜名次，绝不能被当成作品 ID
        assert_eq!(
            parse_pid("#1 - タイトル - 作者 - [pid=12345678] -"),
            Some(12345678)
        );
    }
}
