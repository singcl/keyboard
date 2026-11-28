// 其他平台占位：暂不支持开机自启。

#[cfg(not(any(windows, target_os = "macos")))]

/// 当前平台恒为 false。
pub fn enabled() -> bool {
    false
}

/// 当前平台暂未实现。
pub fn set(_enabled: bool) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "autostart: 当前平台暂不支持开机自启",
    ))
}
