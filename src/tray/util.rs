// 界面小工具：宽字符串转换、弹窗、DPI 缩放、窗口定位。

use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION,
    MB_OK, MB_YESNO, MESSAGEBOX_RESULT, MESSAGEBOX_STYLE, SET_WINDOW_POS_FLAGS,
    SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
};

/// 以 null 结尾的 UTF-16 字符串。
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(crate) fn pcw(v: &[u16]) -> PCWSTR {
    PCWSTR(v.as_ptr())
}

pub(crate) fn msg_box(parent: Option<HWND>, title: &str, text: &str, style: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    let t = wide(title);
    let m = wide(text);
    // SAFETY: 宽字符串均在调用期间有效。
    unsafe { MessageBoxW(parent, pcw(&m), pcw(&t), style) }
}

pub(crate) fn msg_box_error(parent: Option<HWND>, title: &str, text: &str) {
    msg_box(parent, title, text, MB_OK | MB_ICONERROR);
}

pub(crate) fn msg_box_info(parent: Option<HWND>, title: &str, text: &str) {
    msg_box(parent, title, text, MB_OK | MB_ICONINFORMATION);
}

pub(crate) fn msg_box_question(parent: Option<HWND>, title: &str, text: &str) -> bool {
    msg_box(parent, title, text, MB_YESNO | MB_ICONQUESTION) == MESSAGEBOX_RESULT(6) // IDYES
}

// 把窗口放到当前前台窗口所在显示器的中央（Go 版 centerDialog 的逻辑）。
pub(crate) fn center_on_foreground(hwnd: HWND) {
    // SAFETY: 结构体先填 cbSize；句柄均有效。
    unsafe {
        let mut size = windows::Win32::Foundation::RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut size);
        let (w, h) = (size.right - size.left, size.bottom - size.top);
        if w <= 0 || h <= 0 {
            return;
        }

        let mut mon = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTONEAREST);
        if mon.is_invalid() {
            mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        }
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if mon.is_invalid() || !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return;
        }

        let work = &mi.rcWork;
        let x = work.left + (work.right - work.left - w) / 2;
        let y = work.top + (work.bottom - work.top - h) / 2;
        let _ = SetWindowPos(
            hwnd,
            None,
            x,
            y,
            0,
            0,
            SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0 | SWP_NOZORDER.0 | 0x0001), // SWP_NOSIZE
        );
    }
}
