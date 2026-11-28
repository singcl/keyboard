// 内置日志查看窗口：只读文本 + 刷新/清空/关闭按钮，每 2 秒自动刷新。
//
// 对应 Go 版 logview.go；tailLog 的块式反向读取逻辑逐行移植。

use std::cell::{Cell, RefCell};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetStockObject, GetSysColorBrush, COLOR_WINDOW, DEFAULT_GUI_FONT, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{EM_SETREADONLY, EM_SETSEL, EM_SCROLLCARET};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect,
    GetDlgItem, GetWindowLongPtrW, GetWindowTextLengthW, KillTimer,
    LoadCursorW, RegisterClassExW, SendMessageW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, SET_WINDOW_POS_FLAGS,
    SWP_NOZORDER, SW_SHOW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_SETFONT, WM_SIZE, WM_TIMER, WNDCLASS_STYLES,
    WNDCLASSEXW, ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY, GWLP_WNDPROC, IDC_ARROW, SWP_NOMOVE,
    WS_BORDER, WS_CAPTION, WS_CHILDWINDOW, WS_EX_CLIENTEDGE, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
    WS_SYSMENU, WS_TABSTOP, WS_VSCROLL, WS_VISIBLE,
};

use super::util;
use crate::logging;

const LOG_TAIL_LINES: usize = 500;

const ID_TEXT: i32 = 200;
const ID_REFRESH: i32 = 201;
const ID_CLEAR: i32 = 202;
const ID_CLOSE: i32 = 203;
const TIMER_ID: usize = 1;

thread_local! {
    // 只允许同时开一个日志窗口（与 Go 版 logViewOpen 一致）
    static LOG_HWND: RefCell<Option<HWND>> = const { RefCell::new(None) };
    // 每个窗口的私有状态：EDIT 句柄与上次显示的文本
    static STATE: RefCell<Option<LogState>> = const { RefCell::new(None) };
    // EDIT 子类化前的原过程
    static TEXT_PROC: Cell<isize> = const { Cell::new(0) };
}

struct LogState {
    text: Option<HWND>,
    last: String,
}

/// 打开日志窗口；已打开时刷新内容并置于前台。
pub fn show(parent: HWND) {
    let existing = LOG_HWND.with(|slot| slot.borrow().as_ref().copied());
    if let Some(hwnd) = existing {
        reload(hwnd);
        // SAFETY: 句柄有效。
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
        return;
    }

    match create(parent) {
        Ok(hwnd) => {
            LOG_HWND.with(|slot| *slot.borrow_mut() = Some(hwnd));
            reload(hwnd);
            // SAFETY: 句柄有效。
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(Some(hwnd), TIMER_ID, 2000, None);
            }
        }
        Err(e) => util::msg_box_error(Some(parent), "打开日志失败", &e),
    }
}

fn create(parent: HWND) -> Result<HWND, String> {
    // SAFETY: 窗口与控件创建、字体设置均为常规调用。
    unsafe {
        let hinstance = GetModuleHandleW(None).map_err(|e| e.to_string())?;

        let class_name = util::wide("kb-plus-log-view");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: WNDCLASS_STYLES(0),
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).map_err(|e| e.to_string())?,
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(GetSysColorBrush(COLOR_WINDOW).0),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc); // 重复注册忽略

        let title = util::wide("键盘防误触 · 拦截日志");
        let hwnd = CreateWindowExW(
            Default::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WINDOW_STYLE((WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0) as u32),
            0, 0, 820, 480,
            Some(parent),
            None,
            Some(hinstance.into()),
            None,
        )
        .map_err(|e| e.to_string())?;

        let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96) as i32;
        let s = |v: i32| v * dpi / 96;

        let edit_class = util::wide("EDIT");
        let button_class = util::wide("BUTTON");
        let text = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_CLIENTEDGE.0),
            PCWSTR(edit_class.as_ptr()),
            PCWSTR::null(),
            WINDOW_STYLE(
                (WS_CHILDWINDOW.0 | WS_VISIBLE.0 | WS_VSCROLL.0 | WS_BORDER.0) as u32
                    | ES_AUTOVSCROLL as u32
                    | ES_MULTILINE as u32
                    | ES_READONLY as u32,
            ),
            10, 10, s(820 - 20), s(480 - 80),
            Some(hwnd), Some(hmenu(ID_TEXT)), Some(hinstance.into()), None,
        )
        .map_err(|e| e.to_string())?;

        let font: HGDIOBJ = GetStockObject(DEFAULT_GUI_FONT);
        for (id, name) in [(ID_REFRESH, "刷新"), (ID_CLEAR, "清空"), (ID_CLOSE, "关闭")] {
            let t = util::wide(name);
            let b = CreateWindowExW(
                Default::default(),
                PCWSTR(button_class.as_ptr()),
                PCWSTR(t.as_ptr()),
                WINDOW_STYLE((WS_CHILDWINDOW.0 | WS_VISIBLE.0 | WS_TABSTOP.0) as u32),
                0, 0, s(75), s(25),
                Some(hwnd), Some(hmenu(id)), Some(hinstance.into()), None,
            )
            .map_err(|e| e.to_string())?;
            let _ = SendMessageW(b, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        }
        let _ = SendMessageW(text, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        // 只读属性在创建样式里已带，双保险再设置一次（Go 版 SetReadOnly(true)）
        let _ = SendMessageW(text, EM_SETREADONLY, Some(WPARAM(1)), Some(LPARAM(0)));

        // 子类化 EDIT：保留默认行为，拦截回车避免误触发默认按钮
        let proc = GetWindowLongPtrW(text, GWLP_WNDPROC);
        TEXT_PROC.set(proc);
        let proc_fn: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT = text_proc;
        let _ = SetWindowLongPtrW(text, GWLP_WNDPROC, proc_fn as isize);

        // 窗口按 DPI 放大后布局控件
        let _ = SetWindowPos(
            hwnd, None, 0, 0, s(820), s(480),
            SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOZORDER.0),
        );
        layout(hwnd);
        let _ = SetFocus(Some(text));
        // 记录状态
        STATE.with(|slot| {
            *slot.borrow_mut() = Some(LogState { text: Some(text), last: String::new() });
        });
        Ok(hwnd)
    }
}

// 底部按钮靠右排列，文本框占据其余空间（Go 版布局的手工版）。
fn layout(hwnd: HWND) {
    // SAFETY: 句柄有效。
    unsafe {
        let text = STATE.with(|slot| slot.borrow().as_ref().and_then(|s| s.text));
        let Some(text) = text else { return };

        let mut rc = windows::Win32::Foundation::RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        if w <= 0 || h <= 0 {
            return;
        }
        let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96) as i32;
        let s = |v: i32| v * dpi / 96;
        let btn_w = s(75);
        let btn_h = s(25);
        let gap = s(8);
        let margin = s(10);

        let _ = SetWindowPos(text, None, margin, margin, w - margin * 2, h - btn_h - margin * 3, SET_WINDOW_POS_FLAGS(SWP_NOZORDER.0));
        let y = h - btn_h - margin;
        let mut x = w - margin - btn_w;
        // 关闭、清空、刷新 从右往左
        for id in [ID_CLOSE, ID_CLEAR, ID_REFRESH] {
            if let Ok(b) = GetDlgItem(Some(hwnd), id) {
                let _ = SetWindowPos(b, None, x, y, btn_w, btn_h, SET_WINDOW_POS_FLAGS(SWP_NOZORDER.0));
            }
            x -= btn_w + gap;
        }
    }
}

// SAFETY: UI 线程回调；日志窗口状态在线程局部存储。
unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: 系统回调；窗口与控件句柄均为本模块创建。
    unsafe { match msg {
        WM_SIZE => {
            layout(hwnd);
            LRESULT(0)
        }
        WM_TIMER => {
            reload(hwnd);
            LRESULT(0)
        }
        WM_COMMAND => match (wparam.0 as i32) & 0xFFFF {
            ID_REFRESH => {
                reload(hwnd);
                LRESULT(0)
            }
            ID_CLEAR => {
                if util::msg_box_question(Some(hwnd), "清空日志", "确定要清空拦截日志吗？") {
                    if let Err(e) = logging::clear() {
                        util::msg_box_error(Some(hwnd), "清空日志失败", &e.to_string());
                        return LRESULT(0);
                    }
                    reload(hwnd);
                }
                LRESULT(0)
            }
            ID_CLOSE => {
                let _ = SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
                LRESULT(0)
            }
            _ => LRESULT(0),
        },
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
            WM_DESTROY => {
                let _ = KillTimer(Some(hwnd), TIMER_ID);
                LOG_HWND.with(|slot| *slot.borrow_mut() = None);
                STATE.with(|slot| *slot.borrow_mut() = None);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

// SAFETY: EDIT 子类化过程；只读窗口不需要额外交互处理。
unsafe extern "system" fn text_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: 系统回调；转发到子类化前的原过程。
    unsafe {
        let prev = TEXT_PROC.get();
        CallWindowProcW(
            Some(std::mem::transmute::<isize, unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>(prev)),
            hwnd, msg, wparam, lparam,
        )
    }
}

// 读取日志尾部并显示，内容未变化时不重设文本以保留滚动位置。
fn reload(hwnd: HWND) {
    let text = tail_log(LOG_TAIL_LINES);
    let changed = STATE.with(|slot| {
        let mut changed = false;
        if let Some(s) = slot.borrow_mut().as_mut() {
            if s.last != text {
                s.last = text.clone();
                changed = true;
            }
        }
        changed
    });
    if !changed {
        return;
    }
    // SAFETY: 句柄有效；设置文本后滚动到底部。
    unsafe {
        if let Ok(edit) = GetDlgItem(Some(hwnd), ID_TEXT) {
            let w = util::wide(&text);
            let _ = SetWindowTextW(edit, PCWSTR(w.as_ptr()));
            let len = GetWindowTextLengthW(edit);
            let _ = SendMessageW(edit, EM_SETSEL, Some(WPARAM(len as usize)), Some(LPARAM(len as isize)));
            let _ = SendMessageW(edit, EM_SCROLLCARET, Some(WPARAM(0)), Some(LPARAM(0)));
        }
    }
}

// 只从文件尾部按块读取，返回最后 max_lines 行；文件不存在或为空时给出提示。
fn tail_log(max_lines: usize) -> String {
    let Ok(path) = logging::path() else {
        return "（无法定位日志文件）".into();
    };
    tail_from(&path, max_lines)
}

fn tail_from(path: &Path, max_lines: usize) -> String {
    use std::fs::File;

    let Ok(mut f) = File::open(path) else {
        return "（暂无日志）".into();
    };
    let Ok(meta) = f.metadata() else {
        return "（暂无日志）".into();
    };
    if meta.len() == 0 {
        return "（暂无日志）".into();
    }

    const CHUNK: u64 = 32 * 1024;
    let mut data: Vec<u8> = Vec::new();
    let mut offset = meta.len();
    let truncated;
    while offset > 0 {
        let n = CHUNK.min(offset);
        offset -= n;

        let mut buf = vec![0u8; n as usize];
        if f.seek(SeekFrom::Start(offset)).is_err() || f.read_exact(&mut buf).is_err() {
            return "（读取日志失败）".into();
        }
        // 把刚读出的块拼到最前面（反向读取）
        let mut merged = buf;
        merged.extend_from_slice(&data);
        data = merged;

        if data.iter().filter(|&&b| b == b'\n').count() > max_lines {
            break;
        }
    }
    truncated = offset > 0;

    // 未读到文件开头时，首行可能是被截断的半行，丢弃它
    if truncated {
        let Some(i) = data.iter().position(|&b| b == b'\n') else {
            return "（暂无日志）".into();
        };
        data.drain(..=i);
    }

    while data.last() == Some(&b'\n') || data.last() == Some(&b'\r') {
        data.pop();
    }
    if data.is_empty() {
        return "（暂无日志）".into();
    }

    let content = String::from_utf8_lossy(&data);
    let mut lines: Vec<&str> = content.split('\n').collect();
    // 去掉每行结尾的 \r
    for l in lines.iter_mut() {
        *l = l.strip_suffix('\r').unwrap_or(l);
    }
    if lines.len() > max_lines {
        lines = lines[lines.len() - max_lines..].to_vec();
    }
    lines.join("\r\n")
}

#[allow(non_snake_case)]
fn hmenu(id: i32) -> windows::Win32::UI::WindowsAndMessaging::HMENU {
    windows::Win32::UI::WindowsAndMessaging::HMENU(id as *mut _)
}
