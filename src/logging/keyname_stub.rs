// 其他平台暂无按键名表，返回空字符串（日志里显示为 -）。

#[cfg(not(any(windows, target_os = "macos")))]

pub(crate) fn key_name(_vk: u32, _scan_code: u32, _extended: bool) -> String {
    String::new()
}
