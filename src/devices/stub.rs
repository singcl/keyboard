// 非 Windows 平台暂未实现设备枚举。

#[cfg(not(windows))]

/// 当前平台不支持设备枚举。
#[allow(dead_code)]
pub fn list() -> Result<Vec<super::Device>, String> {
    Err(super::ERR_UNSUPPORTED.to_string())
}
