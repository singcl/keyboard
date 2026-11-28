// 自定义防抖阈值对话框（模态）：输入 10-1000 毫秒。
//
// 对应 Go 版 dialog.go 的 askDebounce：带 owner 的模态对话框，Enter 确认、Esc 取消。

use std::cell::Cell;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetStockObject, GetSysColorBrush, COLOR_WINDOW, DEFAULT_GUI_FONT, HBRUSH, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{EM_LIMITTEXT, EM_SETSEL, EM_SCROLLCARET};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, SetFocus, VK_ESCAPE, VK_RETURN, VK_TAB,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, DefWindowProcW, DestroyWindow, GetDlgItem,
    GetParent, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, LoadCursorW,
    MB_ICONQUESTION, MB_YESNOCANCEL, MESSAGEBOX_RESULT, MessageBoxW, PostMessageW,
    RegisterClassExW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos,
    SetWindowTextW, ShowWindow, DispatchMessageW, GetMessageW, ES_AUTOHSCROLL, ES_NUMBER,
    GWLP_WNDPROC, MB_ICONWARNING, MB_OK, MSG, SET_WINDOW_POS_FLAGS, SHOW_WINDOW_CMD,
    SWP_NOZORDER, SW_SHOWNORMAL, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND,
    WM_KEYDOWN, WM_SETFONT, WNDCLASS_STYLES, WNDCLASSEXW, WS_BORDER, WS_CAPTION,
    WS_CHILDWINDOW, WS_EX_CLIENTEDGE, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use super::util;

const WM_APP_DONE: u32 = 0x8010; // WM_APP + 16

const ID_LABEL: i32 = 100;
const ID_EDIT: i32 = 101;
const ID_OK: i32 = 1;
const ID_CANCEL: i32 = 2;

thread_local! {
    // 对话框结果与关闭标记；同一线程同一时刻只有一个模态对话框
    static RESULT: Cell<Option<i32>> = const { Cell::new(None) };
    static DONE: Cell<bool> = const { Cell::new(false) };
    // EDIT 原窗口过程，用于子类化捕获 Enter/Esc/Tab
    static EDIT_PROC: Cell<isize> = const { Cell::new(0) };
    static DLG_HWND: Cell<isize> = const { Cell::new(0) };
}

/// 「关于作者」对话框：版本、作者与链接；样式与「检查更新」弹窗一致（标准消息框）。
/// 「是」打开个人主页，「否」打开项目主页，× / 取消仅关闭。
pub fn show_about(owner: HWND) {
    let msg = format!(
        "keyboard-plus v{}
键盘防抖工具 · MIT License
作者：{}

GitHub：https://github.com/singcl
个人主页：https://imcoco.top

「是」打开个人主页，「否」打开项目主页",
        crate::APP_VERSION,
        crate::AUTHOR
    );
    let choice = util::msg_box(Some(owner), "关于作者", &msg, MB_YESNOCANCEL | MB_ICONQUESTION);
    if choice == MESSAGEBOX_RESULT(6) {
        // IDYES：个人主页
        open_url(Some(owner), crate::AUTHOR_SITE_URL);
    } else if choice == MESSAGEBOX_RESULT(7) {
        // IDNO：项目主页
        open_url(Some(owner), crate::PROJECT_URL);
    }
    // IDCANCEL / ×：仅关闭
}

/// 用 ShellExecuteW 打开 URL / 路径（与 mod.rs 打开配置文件位置同一模式）。
pub fn open_url(owner: Option<HWND>, url: &str) {
    let target = util::wide(url);
    // SAFETY: 目标字符串在调用期间有效。
    let r = unsafe {
        ShellExecuteW(
            owner,
            PCWSTR(w!("open").as_ptr()),
            util::pcw(&target),
            None,
            None,
            SHOW_WINDOW_CMD(SW_SHOWNORMAL.0),
        )
    };
    if r.0 as usize <= 32 {
        util::msg_box_error(owner, "打开链接失败", &format!("无法打开：{url}"));
    }
}

/// 弹出自定义阈值对话框，返回用户确认的毫秒数；取消时返回 None。
pub fn show_threshold(owner: HWND, current: i32) -> Option<i32> {
    RESULT.set(None);
    DONE.set(false);

    let hwnd = match create_dialog(owner, current) {
        Ok(h) => h,
        Err(_) => return None,
    };

    // 模态：禁用 owner，跑嵌套消息循环
    // SAFETY: EnableWindow 与后续窗口操作均为常规调用。
    unsafe {
        let _ = EnableWindow(owner, false);
    }
    // SAFETY: 常规模态对话框循环；DONE 置位后退出。
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        if let Ok(edit) = GetDlgItem(Some(hwnd), ID_EDIT) {
            let _ = SetFocus(Some(edit));
        }
        let mut m = MSG::default();
        while !DONE.get() {
            if GetMessageW(&mut m, None, 0, 0).0 <= 0 {
                break;
            }
            DispatchMessageW(&m);
        }
        let _ = DestroyWindow(hwnd);
        let _ = EnableWindow(owner, true);
        let _ = SetForegroundWindow(owner);
    }
    RESULT.get()
}

fn create_dialog(owner: HWND, current: i32) -> Result<HWND, String> {
    // SAFETY: 窗口类、控件创建与布局均为常规调用。
    unsafe {
        let hinstance = GetModuleHandleW(None).map_err(|e| e.to_string())?;

        let class_name = util::wide("kb-plus-threshold-dlg");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: WNDCLASS_STYLES(0),
            lpfnWndProc: Some(dlg_proc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, windows::Win32::UI::WindowsAndMessaging::IDC_ARROW)
                .map_err(|e| e.to_string())?,
            hbrBackground: HBRUSH(GetSysColorBrush(COLOR_WINDOW).0),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc); // 重复注册忽略

        let title = util::wide("自定义防抖阈值");
        let dlg = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WINDOW_STYLE((WS_CAPTION.0 | WS_SYSMENU.0) as u32),
            0,
            0,
            340,
            150,
            Some(owner),
            None,
            Some(hinstance.into()),
            None,
        )
        .map_err(|e| e.to_string())?;

        DLG_HWND.set(dlg.0 as isize);
        let font: HGDIOBJ = GetStockObject(DEFAULT_GUI_FONT);

        // 控件（96 DPI 布局，创建后按实际 DPI 缩放）
        let label_text =
            util::wide("同一颗键在多少毫秒内重新按下视为误触发（10-1000）：");
        let edit_text = util::wide(&current.to_string());
        let ok_text = util::wide("确定");
        let cancel_text = util::wide("取消");
        let static_class = util::wide("STATIC");
        let edit_class = util::wide("EDIT");
        let button_class = util::wide("BUTTON");

        let label = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(static_class.as_ptr()),
            PCWSTR(label_text.as_ptr()),
            WINDOW_STYLE((WS_CHILDWINDOW.0 | WS_VISIBLE.0) as u32),
            16, 14, 300, 30,
            Some(dlg), Some(hmenu(ID_LABEL)), Some(hinstance.into()), None,
        )
        .map_err(|e| e.to_string())?;

        let edit = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_CLIENTEDGE.0),
            PCWSTR(edit_class.as_ptr()),
            PCWSTR::null(),
            WINDOW_STYLE(
                (WS_CHILDWINDOW.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | WS_BORDER.0) as u32
                    | ES_AUTOHSCROLL as u32
                    | ES_NUMBER as u32,
            ),
            16, 52, 300, 24,
            Some(dlg), Some(hmenu(ID_EDIT)), Some(hinstance.into()), None,
        )
        .map_err(|e| e.to_string())?;
        let _ = SetWindowTextW(edit, PCWSTR(edit_text.as_ptr()));
        let _ = SendMessageW(edit, EM_LIMITTEXT, Some(WPARAM(4)), Some(LPARAM(0)));

        let ok = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(button_class.as_ptr()),
            PCWSTR(ok_text.as_ptr()),
            WINDOW_STYLE((WS_CHILDWINDOW.0 | WS_VISIBLE.0 | WS_TABSTOP.0) as u32),
            160, 96, 75, 25,
            Some(dlg), Some(hmenu(ID_OK)), Some(hinstance.into()), None,
        )
        .map_err(|e| e.to_string())?;

        let cancel = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(button_class.as_ptr()),
            PCWSTR(cancel_text.as_ptr()),
            WINDOW_STYLE((WS_CHILDWINDOW.0 | WS_VISIBLE.0 | WS_TABSTOP.0) as u32),
            242, 96, 75, 25,
            Some(dlg), Some(hmenu(ID_CANCEL)), Some(hinstance.into()), None,
        )
        .map_err(|e| e.to_string())?;

        for h in [label, edit, ok, cancel] {
            let _ = SendMessageW(h, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        }

        // 子类化 EDIT：Enter 确认、Esc 取消
        let proc = GetWindowLongPtrW(edit, GWLP_WNDPROC);
        EDIT_PROC.set(proc);
        let proc_fn: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT = edit_proc;
        let _ = SetWindowLongPtrW(edit, GWLP_WNDPROC, proc_fn as isize);

        // DPI 缩放后居中显示
        scale_dialog(dlg);
        util::center_on_foreground(dlg);
        Ok(dlg)
    }
}

// 编辑框扩展样式见 windows crate 的 ES_* 常量（WindowsAndMessaging 模块）

// 把 96 DPI 的布局按窗口实际 DPI 放大。
fn scale_dialog(dlg: HWND) {
    // SAFETY: 句柄有效。
    unsafe {
        let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(dlg).max(96) as i32;
        if dpi == 96 {
            return;
        }
        let s = |v: i32| v * dpi / 96;
        let mut rect = windows::Win32::Foundation::RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(dlg, &mut rect);
        let w = s(rect.right - rect.left);
        let h = s(rect.bottom - rect.top);
        let _ = SetWindowPos(
            dlg, None, 0, 0, w, h,
            SET_WINDOW_POS_FLAGS(
                windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE.0 | SWP_NOZORDER.0,
            ),
        );
        for (id, x, y, cw, ch) in [
            (ID_LABEL, 16, 14, 300, 30),
            (ID_EDIT, 16, 52, 300, 24),
            (ID_OK, 160, 96, 75, 25),
            (ID_CANCEL, 242, 96, 75, 25),
        ] {
            if let Ok(hctl) = GetDlgItem(Some(dlg), id) {
                let _ = SetWindowPos(hctl, None, s(x), s(y), s(cw), s(ch), SET_WINDOW_POS_FLAGS(SWP_NOZORDER.0));
            }
        }
    }
}

// SAFETY: UI 线程回调；线程局部状态保存对话框上下文。
unsafe extern "system" fn dlg_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => match (wparam.0 as i32) & 0xFFFF {
            ID_OK => try_accept(hwnd),
            ID_CANCEL => finish(None),
            _ => {}
        },
        WM_CLOSE => finish(None),
        WM_APP_DONE => {
            DONE.set(true);
        }
        _ => {}
    }
    let _ = lparam;
    // SAFETY: 其余消息交还默认处理。
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

// SAFETY: EDIT 子类化过程；转发到原过程处理其余消息。
unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: 系统回调；EDIT 由本模块创建并子类化。
    unsafe {
        if msg == WM_KEYDOWN {
            let vk = wparam.0 as u32 as u16;
            if vk == VK_RETURN.0 {
                if let Ok(dlg) = GetParent(hwnd) {
                    try_accept(dlg);
                }
                return LRESULT(0);
            }
            if vk == VK_ESCAPE.0 {
                finish(None);
                return LRESULT(0);
            }
            if vk == VK_TAB.0 {
                // 单输入框：Tab 保持焦点
                let _ = SetFocus(Some(hwnd));
                return LRESULT(0);
            }
        }
        let prev = EDIT_PROC.get();
        CallWindowProcW(
            Some(std::mem::transmute::<
                isize,
                unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
            >(prev)),
            hwnd, msg, wparam, lparam,
        )
    }
}

fn try_accept(dlg: HWND) {
    // SAFETY: 句柄有效；读取 EDIT 文本。
    unsafe {
        let Ok(edit) = GetDlgItem(Some(dlg), ID_EDIT) else { return };
        let len = GetWindowTextLengthW(edit);
        let mut buf = vec![0u16; (len + 1) as usize];
        let n = GetWindowTextW(edit, &mut buf);
        let text = String::from_utf16_lossy(&buf[..n as usize]).trim().to_string();

        match text.parse::<i32>() {
            Ok(ms) if (10..=1000).contains(&ms) => {
                RESULT.set(Some(ms));
                finish(Some(ms));
            }
            _ => {
                let msg = util::wide("请输入 10-1000 之间的整数。");
                let title = util::wide("数值无效");
                let _ = MessageBoxW(
                    Some(dlg),
                    PCWSTR(msg.as_ptr()),
                    PCWSTR(title.as_ptr()),
                    MB_OK | MB_ICONWARNING,
                );
                let _ = SetFocus(Some(edit));
                // 光标移到末尾
                let elen = GetWindowTextLengthW(edit);
                let _ = SendMessageW(edit, EM_SETSEL, Some(WPARAM(elen as usize)), Some(LPARAM(elen as isize)));
                let _ = SendMessageW(edit, EM_SCROLLCARET, Some(WPARAM(0)), Some(LPARAM(0)));
            }
        }
    }
}

fn finish(value: Option<i32>) {
    if let Some(v) = value {
        RESULT.set(Some(v));
    }
    DONE.set(true);
    // 退出模态循环（窗口由外层统一销毁）
    let h = DLG_HWND.get();
    if h != 0 {
        // SAFETY: 消息仅在对话框窗口上投递。
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_APP_DONE, WPARAM(0), LPARAM(0));
        }
    }
}

// HMENU 的简写构造
#[allow(non_snake_case)]
fn hmenu(id: i32) -> windows::Win32::UI::WindowsAndMessaging::HMENU {
    windows::Win32::UI::WindowsAndMessaging::HMENU(id as *mut _)
}
