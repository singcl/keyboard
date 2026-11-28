// 标准版（带 update feature）的在线检查实现。

#[cfg(feature = "update")]

use std::io::Read;
use std::time::Duration;

use serde::Deserialize;

use super::Release;

const LATEST_URL: &str = "https://api.github.com/repos/singcl/keyboard/releases/latest";
const MAX_BODY: u64 = 1 << 20; // 与 Go 版 LimitReader(1MB) 一致

#[derive(Deserialize)]
struct ReleaseBody {
    tag_name: String,
    html_url: String,
}

/// 查询 GitHub 上的最新发布版本。
pub fn latest() -> Result<Release, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .new_agent();

    let response = agent
        .get(LATEST_URL)
        .header("User-Agent", "keyboard-plus")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("GitHub 请求失败：{e}"))?;

    if response.status() != 200 {
        return Err(format!("GitHub 返回 {}", response.status()));
    }

    let mut body = response.into_body();
    let mut text = String::new();
    body
        .as_reader()
        .take(MAX_BODY)
        .read_to_string(&mut text)
        .map_err(|e| format!("读取响应失败：{e}"))?;

    let rel: ReleaseBody = serde_json::from_str(&text).map_err(|e| format!("解析发布信息失败：{e}"))?;
    if rel.tag_name.is_empty() || rel.html_url.is_empty() {
        return Err("发布信息不完整".to_string());
    }
    Ok(Release { tag: rel.tag_name, url: rel.html_url })
}
