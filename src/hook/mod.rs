// Package hook 拦截键盘误触发的重复按键。
//
// 原理：如果同一颗按键在没有抬起的情况下再次收到按下，说明是按住不放的自动重复，放行；
// 如果抬起后又在很短时间内被重新按下，说明是键盘误触发的重复响应，直接拦截。
//
// 拦截能力由各平台实现提供：Windows 见 hook::windows（低级键盘钩子），
// macOS 见 hook::macos（CGEventTap），其他平台目前为占位实现。
// 判重逻辑（should_block）与防抖/暂停状态是平台无关的。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};
use std::time::Duration;

pub type BlockHandler = Arc<dyn Fn(u32, u32, bool, Duration) + Send + Sync>;

#[cfg(windows)]
mod windows;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(windows, target_os = "macos")))]
mod stub;

mod imp;

#[allow(unused_imports)]
pub use imp::{setup, stop};

// 最近一次按下的状态与防抖/暂停标志。
// 全部用原子量：写入方只有钩子回调线程（单线程写、多线程读），无需互斥锁。
static LAST_VK: AtomicU32 = AtomicU32::new(0);
static LAST_DOWN_NANOS: AtomicI64 = AtomicI64::new(0);
static DELAY_NANOS: AtomicI64 = AtomicI64::new(100_000_000); // 默认 10 次/秒
static PAUSED: AtomicBool = AtomicBool::new(false);

// 记录每个按键当前是否处于按下状态，用于区分「自动重复」与「重新按下」。
// 仅 Windows 回调使用；macOS 的 CGEvent 直接给出 autoRepeat 标志。
#[cfg_attr(not(windows), allow(dead_code))]
static KEY_DOWN: [AtomicBool; 256] = {
    #[allow(clippy::declare_interior_mutable_const)]
    const FALSE: AtomicBool = AtomicBool::new(false);
    [FALSE; 256]
};

/// 设置同一颗按键两次「重新按下」之间的最小间隔，<=0 时忽略。
pub fn set_debounce(d: Duration) {
    let nanos = d.as_nanos() as i64;
    if nanos <= 0 {
        return;
    }
    DELAY_NANOS.store(nanos, Ordering::Relaxed);
}

/// 判断一次按下是否属于需要拦截的误触发：
/// 同键、非自动重复，且距上次按下小于防抖窗口。
pub fn should_block(auto_repeat: bool, vk: u32, prev_vk: u32, since: Duration, debounce: Duration) -> bool {
    if auto_repeat || prev_vk == 0 || vk != prev_vk {
        return false;
    }
    since < debounce
}

/// 暂停或恢复拦截；暂停期间所有按键直接放行。
pub fn set_paused(pause: bool) {
    PAUSED.store(pause, Ordering::Relaxed);
}

pub(crate) fn paused() -> bool {
    PAUSED.load(Ordering::Relaxed)
}

/// 在按键抬起时清理按下状态，各平台实现共用。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn record_key_up(vk: u32) {
    if vk < 256 {
        KEY_DOWN[vk as usize].store(false, Ordering::Relaxed);
    }
}

/// 判断本次按下是否需要拦截，并更新「最近一次按下」的基准；
/// 需要拦截时返回距上次按下的间隔。各平台实现共用这套判重逻辑。
pub(crate) fn should_block_now(vk: u32) -> Option<Duration> {
    let now = crate::timeutil::unix_nanos();
    let delay = DELAY_NANOS.load(Ordering::Relaxed).max(0) as u128;
    let prev_vk = LAST_VK.load(Ordering::Relaxed);
    let last_down = LAST_DOWN_NANOS.load(Ordering::Relaxed);

    if last_down > 0 && prev_vk != 0 {
        let since = (now - last_down).max(0) as u128;
        if should_block(false, vk, prev_vk, Duration::from_nanos(since as u64), Duration::from_nanos(delay as u64)) {
            return Some(Duration::from_nanos(since as u64));
        }
    }

    LAST_DOWN_NANOS.store(now, Ordering::Relaxed);
    LAST_VK.store(vk, Ordering::Relaxed);
    None
}

/// 读出某键当前是否处于按下状态（供 Windows 回调区分自动重复）。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn key_is_down(vk: u32) -> bool {
    vk < 256 && KEY_DOWN[vk as usize].load(Ordering::Relaxed)
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn set_key_down(vk: u32, down: bool) {
    if vk < 256 {
        KEY_DOWN[vk as usize].store(down, Ordering::Relaxed);
    }
}

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    LAST_VK.store(0, Ordering::Relaxed);
    LAST_DOWN_NANOS.store(0, Ordering::Relaxed);
    for kd in KEY_DOWN.iter() {
        kd.store(false, Ordering::Relaxed);
    }
    set_debounce(Duration::from_millis(100));
    set_paused(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_block_table() {
        let debounce = Duration::from_millis(100);
        let cases = [
            (true, b'A' as u32, b'A' as u32, Duration::from_millis(10), false, "自动重复放行"),
            (false, b'A' as u32, 0, Duration::from_millis(0), false, "首次按下放行"),
            (false, b'B' as u32, b'A' as u32, Duration::from_millis(10), false, "不同按键放行"),
            (false, b'A' as u32, b'A' as u32, Duration::from_millis(50), true, "快速重按拦截"),
            (false, b'A' as u32, b'A' as u32, Duration::from_millis(150), false, "超过窗口放行"),
            (false, b'A' as u32, b'A' as u32, debounce, false, "等于窗口放行"),
        ];
        for (auto_repeat, vk, prev, since, want, name) in cases {
            let got = should_block(auto_repeat, vk, prev, since, debounce);
            assert_eq!(got, want, "{name}");
        }
    }

    // Windows 键盘回调允许直接注入合成事件， pause/block 的行为在这里验证
    #[cfg(windows)]
    #[test]
    fn pause_and_block_via_callback() {
        crate::hook::windows::test_support::call_key(0x0100, 0x41); // WM_KEYDOWN 'A'
        reset_for_tests();

        // 暂停期间不应拦截，也不应记录
        set_paused(true);
        call(0x0100, 0x41);
        call(0x0101, 0x41);
        call(0x0100, 0x41);
        assert_eq!(blocked_count(), 0, "paused");

        // 恢复后，同一按键快速重按应被拦截
        set_paused(false);
        call(0x0100, 0x41);
        call(0x0101, 0x41);
        call(0x0100, 0x41);
        assert_eq!(blocked_count(), 1, "resumed");
    }

    #[cfg(windows)]
    fn call(msg: u32, vk: u32) {
        crate::hook::windows::test_support::call_key(msg, vk);
    }

    #[cfg(windows)]
    fn blocked_count() -> u32 {
        // 回调 handler 由 test_support 注入并统计
        crate::hook::windows::test_support::blocked_count()
    }
}
