// Windows 平台实现：WH_KEYBOARD_LL 低级键盘钩子。
//
// 低级键盘钩子要求安装它的线程必须有消息循环，因此 setup 在当前线程上
// 安装钩子并跑 GetMessage 循环（调用方负责把这个函数放到独立线程上）。

#[cfg(windows)]

use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::OnceLock;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, MessageBoxW, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK,
    KBDLLHOOKSTRUCT, KBDLLHOOKSTRUCT_FLAGS, LLKHF_EXTENDED, MB_ICONERROR, MB_OK, MSG,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use super::{key_is_down, paused, record_key_up, set_key_down, should_block_now, BlockHandler};

static ON_BLOCK: OnceLock<BlockHandler> = OnceLock::new();
// 钩子句柄：安装线程写入，回调用它转发消息；测试注入事件时保持为空
static HOOK_HANDLE: AtomicIsize = AtomicIsize::new(0);
// 最近一次因测试注入而被拦截的次数，仅测试路径使用
#[cfg(test)]
static TEST_BLOCKED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// 安装钩子并在当前线程上跑消息循环，正常情况下不会返回。
pub fn setup(handler: BlockHandler) -> Result<(), String> {
    let _ = ON_BLOCK.set(handler);

    // SAFETY: 安装钩子后立即进入消息循环；低级钩子回调由系统在该线程投递。
    unsafe {
        let h = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0)
            .map_err(|e| format!("hook: 安装键盘钩子失败：{e}"))?;
        HOOK_HANDLE.store(h.0 as isize, Ordering::Relaxed);

        let mut m = MSG::default();
        while GetMessageW(&mut m, None, 0, 0).0 > 0 {}

        let _ = UnhookWindowsHookEx(h);
    }
    Ok(())
}

// 与 macOS 实现保持同名 API（macOS 在退出时移除 tap；Windows 钩子随线程结束释放）
#[allow(dead_code)]
pub fn stop() {}

// SAFETY: 由系统在安装钩子的线程里调用；lParam 指向系统提供的 KBDLLHOOKSTRUCT。
unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: 由系统在安装钩子的线程里调用；lParam 指向系统提供的 KBDLLHOOKSTRUCT。
    let ctx = unsafe {
        if code >= 0 && !paused() {
            Some((&*(lparam.0 as *const KBDLLHOOKSTRUCT), wparam.0 as u32))
        } else {
            None
        }
    };
    if let Some((kb, msg)) = ctx {
        let vk = kb.vkCode;

        match msg {
            WM_KEYUP | WM_SYSKEYUP => record_key_up(vk),
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                // 没有抬起又收到按下，说明是按住不放产生的自动重复
                let auto_repeat = key_is_down(vk);
                set_key_down(vk, true);

                // 自动重复直接放行，避免影响长按删除、长按移动光标等操作
                if !auto_repeat {
                    if let Some(since) = should_block_now(vk) {
                        let extended = (kb.flags & LLKHF_EXTENDED) == KBDLLHOOKSTRUCT_FLAGS(LLKHF_EXTENDED.0);
                        if let Some(handler) = ON_BLOCK.get() {
                            handler(vk, kb.scanCode, extended, since);
                        }
                        #[cfg(test)]
                        TEST_BLOCKED.fetch_add(1, Ordering::Relaxed);
                        return LRESULT(1); // 拦截本次误触发
                    }
                }
            }
            _ => {}
        }
    }

    // SAFETY: CallNextHookEx 允许传入空句柄（Go 版同样传 hookHandle 原值转发）。
    let handle = HHOOK(HOOK_HANDLE.load(Ordering::Relaxed) as *mut std::ffi::c_void);
    unsafe { CallNextHookEx(Some(handle), code, wparam, lparam) }
}

// 仅测试使用：绕过真实钩子，直接驱动回调逻辑。
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub fn call_key(msg: u32, vk: u32) {
        let kb = KBDLLHOOKSTRUCT {
            vkCode: vk,
            scanCode: 0x1E,
            flags: Default::default(),
            time: 0,
            dwExtraInfo: 0,
        };
        // SAFETY: 传入自己构造的结构体指针，回调只读取它。
        unsafe {
            keyboard_proc(0, WPARAM(msg as usize), LPARAM(&kb as *const _ as isize));
        }
    }

    pub fn blocked_count() -> u32 {
        TEST_BLOCKED.load(Ordering::Relaxed)
    }
}

// 未走测试路径时保留 MB_* 引用（安装失败弹窗与 Go 版一致）
#[allow(dead_code)]
fn install_error_box(text: &str) {
    let wide = to_wide(text);
    let caption = to_wide("Error");
    // SAFETY: 弹窗参数均为以 null 结尾的宽字符串。
    unsafe {
        MessageBoxW(None, windows::core::PCWSTR(wide.as_ptr()), windows::core::PCWSTR(caption.as_ptr()), MB_OK | MB_ICONERROR);
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
