// 通过 GitHub Releases 检查是否有新版本。
//
// 只做「检查 + 提示」，不下载、不替换程序：两个平台的产物都未签名，
// 自动替换会触发 SmartScreen / Gatekeeper，macOS 替换后还可能丢失输入监控授权。
//
// 精简版（--no-default-features，即不带 update feature）不链接任何 HTTP/TLS 代码，
// 体积约为标准版的一半，此时 enabled() 为 false，界面不显示「检查更新」。

#[cfg(feature = "update")]
mod latest;

/// 最新发布的信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// 例如 v1.2.4
    pub tag: String,
    /// 发布页面地址
    pub url: String,
}

/// 当前构建是否包含在线检查更新功能。
pub const fn enabled() -> bool {
    cfg!(feature = "update")
}

/// 查询 GitHub 上的最新发布版本；精简版恒返回错误。
pub fn latest() -> Result<Release, String> {
    #[cfg(feature = "update")]
    return latest::latest();

    #[cfg(not(feature = "update"))]
    Err("此版本为精简版，不包含检查更新功能".to_string())
}

/// 判断 latest 是否比 current 新；任一版本号无法解析时返回 false。
pub fn newer(current: &str, latest: &str) -> bool {
    let (Some(c), Some(l)) = (parse(current), parse(latest)) else {
        return false;
    };
    for i in 0..c.len().max(l.len()) {
        let a = c.get(i).copied().unwrap_or(0);
        let b = l.get(i).copied().unwrap_or(0);
        if a != b {
            return b > a;
        }
    }
    false
}

/// 判断字符串是否为可比较的版本号，如 1.2.3 或 v1.2.3。
/// 开发构建（dev-abc1234）等无法解析的版本号返回 false。
pub fn is_version(v: &str) -> bool {
    parse(v).is_some()
}

/// 把 1.2.3 / v1.2.3 解析成数字切片；带前后缀等无法解析时返回 None。
fn parse(v: &str) -> Option<Vec<u32>> {
    let trimmed = v.trim();
    let v = trimmed.strip_prefix('v').unwrap_or(trimmed);
    if v.is_empty() {
        return None;
    }

    v.split('.')
        .map(|p| p.parse::<u32>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_table() {
        let cases = [
            ("1.2.3", "v1.2.4", true),
            ("v1.2.4", "1.2.3", false),
            ("1.2.3", "v1.2.3", false),
            ("1.2", "1.2.0", false),
            ("1.2.0", "1.3", true),
            ("1.9.9", "1.10.0", true),
            ("1.10.0", "1.9.9", false),
            ("dev-abc1234", "v1.2.4", false),
            ("1.2.3", "dev-abc1234", false),
            ("未知", "v1.2.4", false),
        ];
        for (current, latest, want) in cases {
            assert_eq!(newer(current, latest), want, "newer({current:?}, {latest:?})");
        }
    }

    #[test]
    fn is_version_table() {
        for v in ["1.2.3", "v1.2.3", "1.2", " 1.2.3 "] {
            assert!(is_version(v), "IsVersion({v:?}) 应为 true");
        }
        for v in ["", "dev", "dev-abc1234", "1.2.3-beta", "未知"] {
            assert!(!is_version(v), "IsVersion({v:?}) 应为 false");
        }
    }
}
