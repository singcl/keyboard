// 配置持久化：config.json，字段名与 Go 版完全一致，老用户的配置可以直接沿用。
//
// 路径与 Go 版 os.UserCacheDir 相同：Windows 为 %LOCALAPPDATA%\keyboard-plus\，
// macOS 为 ~/Library/Caches/keyboard-plus/。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 默认的防抖窗口（毫秒）。
pub const DEFAULT_DEBOUNCE_MS: i32 = 100;

/// 持久化的程序配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "debounce_ms")]
    pub debounce_ms: i32,

    /// 目标键盘：名称或设备路径匹配任一即可
    #[serde(rename = "target_name", default)]
    pub target_name: String,
    #[serde(rename = "target_path", default)]
    pub target_path: String,

    /// 为 true 时，仅当目标键盘在线才启用防抖
    #[serde(rename = "only_when_connected", default)]
    pub only_when_connected: bool,

    /// 记录用户是否手动暂停了拦截，重启后保持该状态
    #[serde(rename = "paused", default)]
    pub paused: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            debounce_ms: DEFAULT_DEBOUNCE_MS,
            target_name: String::new(),
            target_path: String::new(),
            only_when_connected: false,
            paused: false,
        }
    }
}

/// 返回配置文件路径，并确保所在目录存在。
pub fn path() -> io::Result<PathBuf> {
    path_in(&base_cache_dir()?)
}

pub(crate) fn path_in(base: &Path) -> io::Result<PathBuf> {
    let dir = base.join("keyboard-plus");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("config.json"))
}

// 与 Go 版 os.UserCacheDir 保持一致：Windows 读 %LOCALAPPDATA%，macOS 为 ~/Library/Caches。
pub(crate) fn base_cache_dir() -> io::Result<PathBuf> {
    // 测试专用：把「用户缓存目录」指到临时目录，等价于 Go 版的 t.Setenv("LOCALAPPDATA", ...)
    #[cfg(test)]
    if let Some(dir) = base_override() {
        return Ok(dir);
    }

    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "%LOCALAPPDATA% 未设置"))
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "$HOME 未设置"))?;
        Ok(home.join("Library").join("Caches"))
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "$XDG_CACHE_HOME 未设置"))
    }
}

// 测试专用的用户缓存目录覆盖；仅 cfg(test) 下编译，生产代码路径不受影响。
#[cfg(test)]
static BASE_OVERRIDE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

#[cfg(test)]
fn base_override() -> Option<PathBuf> {
    BASE_OVERRIDE.lock().unwrap().clone()
}

/// 测试专用：重定向用户缓存目录（配置、日志与单实例锁都基于它）。
#[cfg(test)]
pub(crate) fn set_base_dir_for_tests(dir: &Path) {
    *BASE_OVERRIDE.lock().unwrap() = Some(dir.to_path_buf());
}

/// 读取配置；文件不存在或内容非法时返回默认值。
pub fn load() -> Config {
    load_from(&path().unwrap_or_default())
}

pub(crate) fn load_from(path: &Path) -> Config {
    let default = Config::default();
    let Ok(text) = fs::read_to_string(path) else {
        return default;
    };
    let Ok(mut cfg) = serde_json::from_str::<Config>(&text) else {
        return Config::default();
    };
    if !(10..=1000).contains(&cfg.debounce_ms) {
        cfg.debounce_ms = DEFAULT_DEBOUNCE_MS;
    }
    cfg
}

/// 写入配置（两个字段的空串等零值也照写，保持文件结构稳定）。
pub fn save(cfg: &Config) -> io::Result<()> {
    save_to(&path()?, cfg)
}

pub(crate) fn save_to(path: &Path, cfg: &Config) -> io::Result<()> {
    let mut text = serde_json::to_string_pretty(cfg).expect("config 序列化不会失败");
    text.push('\n');
    fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn save_load_roundtrip_and_validation() {
        let dir = tempfile_for_tests();
        let path = path_in(&dir).unwrap();

        // 文件不存在：默认值
        assert_eq!(load_from(&path).debounce_ms, DEFAULT_DEBOUNCE_MS);

        // 正常写入后能读回
        let mut cfg = Config::default();
        cfg.debounce_ms = 67;
        save_to(&path, &cfg).unwrap();
        assert_eq!(load_from(&path).debounce_ms, 67);

        // 非法 JSON：默认值
        fs::write(&path, "not json").unwrap();
        assert_eq!(load_from(&path).debounce_ms, DEFAULT_DEBOUNCE_MS);

        // 超出范围：默认值
        fs::write(&path, r#"{"debounce_ms": 5}"#).unwrap();
        assert_eq!(load_from(&path).debounce_ms, DEFAULT_DEBOUNCE_MS);
    }

    #[test]
    fn json_field_names_unchanged() {
        // 与 Go 版写出的字段名保持一致，老配置必须能读入
        let cfg: Config =
            serde_json::from_str(r#"{"debounce_ms":80,"target_name":"K","target_path":"P","only_when_connected":true,"paused":true}"#).unwrap();
        assert_eq!(cfg.debounce_ms, 80);
        assert_eq!(cfg.target_name, "K");
        assert_eq!(cfg.target_path, "P");
        assert!(cfg.only_when_connected);
        assert!(cfg.paused);

        let text = serde_json::to_string(&Config::default()).unwrap();
        for field in ["debounce_ms", "target_name", "target_path", "only_when_connected", "paused"] {
            assert!(text.contains(field), "missing {field} in {text}");
        }
    }

    fn tempfile_for_tests() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kb-plus-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
