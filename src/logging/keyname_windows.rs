// Windows：通过扫描码获取可读的按键名称（GetKeyNameTextW），失败时返回空字符串。

#[cfg(windows)]

use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyNameTextW;

pub(crate) fn key_name(vk: u32, scan_code: u32, extended: bool) -> String {
    let _ = vk;
    let mut lparam = (scan_code << 16) as i32;
    if extended {
        lparam |= 1 << 24;
    }
    let mut buf = [0u16; 64];
    // SAFETY: 缓冲区在调用期间有效，长度为元素个数。
    let n = unsafe { GetKeyNameTextW(lparam, &mut buf) };
    if n == 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..(n as usize).min(buf.len())])
}
