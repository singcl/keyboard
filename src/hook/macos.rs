// macOS 平台实现：CGEventTap 全局键盘事件监听。
//
// 与 Windows 不同，这里安装完成后立即返回：事件由主线程的 run loop 驱动，
// 因此调用方需要在主线程启动 AppKit 的事件循环（NSApplication run）。
// 返回 Err 表示安装失败，通常是缺少「输入监控」权限。

#[cfg(target_os = "macos")]

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{paused, should_block_now, BlockHandler};

static ON_BLOCK: AtomicI32 = AtomicI32::new(0); // 是否已注入回调（用原子量避免 OnceLock 竞争）

// 回调经 C 层 goHandleKeyCode 转入；为保证与 Go 版同构，C 层调用这里导出的符号。
// handler 本体保存在线程安全的静态槽里。
static HANDLER: std::sync::Mutex<Option<BlockHandler>> = std::sync::Mutex::new(None);

extern "C" {
    fn kbdTapStart() -> i32;
    fn kbdTapStop();
    fn kbdAccessTrusted() -> i32;
    fn kbdAccessPrompt() -> i32;
}

/// 安装全局键盘事件监听（CGEventTap）。
pub fn setup(handler: BlockHandler) -> Result<(), String> {
    *HANDLER.lock().unwrap() = Some(handler);
    ON_BLOCK.store(1, Ordering::Relaxed);

    // SAFETY: 均为无参 C 函数，与 hook_darwin.c 的实现一一对应。
    unsafe {
        // 没有权限时先触发系统的授权引导，用户只需在系统设置里勾选本程序
        if kbdAccessTrusted() == 0 {
            kbdAccessPrompt();
        }

        match kbdTapStart() {
            0 => Ok(()),
            1 => Err("hook: 没有键盘监听权限，请在「系统设置 → 隐私与安全性 → 输入监控」中勾选本程序后重新启动".into()),
            _ => Err("hook: 无法创建键盘事件监听".into()),
        }
    }
}

/// 移除全局键盘事件监听，退出程序前调用。
pub fn stop() {
    // SAFETY: 无参 C 函数。
    unsafe { kbdTapStop() }
}

/// 由 CGEventTap 的回调在主线程调用，返回 1 表示拦截这次按键。
/// keyCode 是 macOS 的虚拟键码，auto_repeat 表示按住不放产生的自动重复。
///
/// # Safety
/// 由 C 层（hook_darwin.c 的 keyboardTapCallback）调用，签名必须与 bridge.h 一致。
#[no_mangle]
pub unsafe extern "C" fn goHandleKeyCode(key_code: u32, down: i32, auto_repeat: i32) -> i32 {
    if paused() || down == 0 || auto_repeat != 0 {
        return 0;
    }
    if let Some(since) = should_block_now(key_code) {
        if let Some(handler) = HANDLER.lock().unwrap().as_ref() {
            let handler = Arc::clone(handler);
            handler(key_code, 0, false, since);
        }
        return 1;
    }
    0
}

// 让 Duration 在此文件被用到（防止未来裁剪 import 时遗漏）
#[allow(unused)]
fn _assert_duration_used(_: Duration) {}
