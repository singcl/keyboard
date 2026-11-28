// macOS 入口：菜单栏图标 + CGEventTap 全局拦截。
//
// 与 Go 版 main_darwin.go 一致：菜单栏与事件监听都必须运行在主线程的 run loop 上，
// 因此入口在主线程上依次初始化 AppKit 和事件监听。

mod ui;

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::assets;
use crate::autostart;
use crate::config::{self, Config};
use crate::hook;
use crate::logging;
use crate::singleinstance;
use crate::stats;
use crate::update;
use crate::APP_VERSION;

// UI 线程与后台共享的状态（与 Go 版的原子量一致）
static USER_PAUSED: AtomicI32 = AtomicI32::new(0);
static TAP_ACTIVE: AtomicI32 = AtomicI32::new(0);

static CFG: Mutex<Option<Config>> = Mutex::new(None);

fn cfg() -> Config {
    CFG.lock().unwrap().clone().unwrap_or_default()
}

fn set_cfg(f: impl FnOnce(&mut Config)) {
    let mut guard = CFG.lock().unwrap();
    if let Some(c) = guard.as_mut() {
        f(c);
        let _ = config::save(c);
    }
}

pub fn main_macos() {
    // 菜单栏与事件监听都必须运行在主线程的 run loop 上
    let (_guard, ok) = singleinstance::acquire("keyboard-plus");
    if !ok {
        ui::alert("键盘防抖", "程序已在运行，请在菜单栏的键盘图标中操作。");
        return;
    }

    let cfg0 = config::load();
    *CFG.lock().unwrap() = Some(cfg0.clone());
    USER_PAUSED.store(bool_to_i32(cfg0.paused), Ordering::Relaxed);

    // 第二套图标样式（脉冲屏）：仅标准版内嵌，精简版传 None（菜单不显示切换项）
    #[cfg(feature = "icon-switch")]
    let (normal_alt, paused_alt): (Option<&[u8]>, Option<&[u8]>) =
        (Some(assets::ICON_ALT_PNG), Some(assets::ICON_ALT_PAUSED_PNG));
    #[cfg(not(feature = "icon-switch"))]
    let (normal_alt, paused_alt): (Option<&[u8]>, Option<&[u8]>) = (None, None);

    ui::init(
        assets::ICON_PNG,
        assets::ICON_PAUSED_PNG,
        normal_alt,
        paused_alt,
        &format!("版本 {APP_VERSION}"),
        update::enabled(),
    );

    // 恢复上次的暂停状态
    hook::set_paused(cfg0.paused);

    let handler: hook::BlockHandler = Arc::new(|vk, scan, ext, interval| {
        stats::add_blocked();
        logging::blocked(vk, scan, ext, interval);
        ui::refresh();
    });
    if let Err(e) = hook::setup(handler) {
        logging::event(&e);
        ui::permission_alert();
    } else {
        TAP_ACTIVE.store(1, Ordering::Relaxed);
    }
    ui::refresh();

    // 启动时静默检查一次更新，只有存在新版本才会提示
    if update::enabled() {
        check_update(false);
    }

    // 事件由主线程 run loop 驱动，正常情况下不会返回
    ui::run();
}

// ---------- 菜单回调（Cocoa 通过 bridge.h 调用） ----------

/// 菜单栏状态：0 运行中、1 已暂停、2 未生效（缺少权限）。
#[unsafe(no_mangle)]
pub extern "C" fn goState() -> i32 {
    if TAP_ACTIVE.load(Ordering::Relaxed) == 0 {
        return 2;
    }
    goPaused()
}

#[unsafe(no_mangle)]
pub extern "C" fn goPaused() -> i32 {
    USER_PAUSED.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn goBlockedToday() -> i32 {
    stats::today_count().min(i32::MAX as i64) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn goDebounceMS() -> i32 {
    cfg().debounce_ms
}

#[unsafe(no_mangle)]
pub extern "C" fn goTogglePause() {
    let now_paused = !cfg().paused;
    set_cfg(|c| c.paused = now_paused);
    USER_PAUSED.store(bool_to_i32(now_paused), Ordering::Relaxed);
    hook::set_paused(now_paused);
    if now_paused {
        logging::event("已手动暂停拦截");
    } else {
        logging::event("已手动恢复拦截");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn goSetDebounce(ms: i32) {
    if !(10..=1000).contains(&ms) {
        ui::alert("数值无效", "请输入 10-1000 之间的整数。");
        return;
    }
    set_cfg(|c| c.debounce_ms = ms);
    hook::set_debounce(Duration::from_millis(ms.max(0) as u64));
    logging::event(&format!("防抖阈值调整为 {ms}ms"));
}

#[unsafe(no_mangle)]
pub extern "C" fn goAutostartEnabled() -> i32 {
    bool_to_i32(autostart::enabled())
}

#[unsafe(no_mangle)]
pub extern "C" fn goToggleAutostart() {
    let enable = !autostart::enabled();
    if let Err(e) = autostart::set(enable) {
        ui::alert("设置开机自启失败", &e.to_string());
        return;
    }
    if enable {
        logging::event("已开启开机自启");
    } else {
        logging::event("已关闭开机自启");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn goOpenLog() {
    let path = match logging::path() {
        Ok(p) => p,
        Err(e) => {
            ui::alert("打开日志失败", &e.to_string());
            return;
        }
    };
    if !path.exists() {
        ui::alert("暂无日志", "目前还没有拦截记录。");
        return;
    }
    let path_str = path.to_string_lossy().to_string();
    // SAFETY: spawn 失败只影响本次打开。
    if std::process::Command::new("/usr/bin/open").arg(&path_str).spawn().is_err() {
        ui::alert("打开日志失败", "无法调用 /usr/bin/open。");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn goRevealConfig() {
    let path = match config::path() {
        Ok(p) => p,
        Err(e) => {
            ui::alert("定位配置文件失败", &e.to_string());
            return;
        }
    };
    let path_str = path.to_string_lossy().to_string();
    // SAFETY: spawn 失败只影响本次定位。
    if std::process::Command::new("/usr/bin/open").arg("-R").arg(&path_str).spawn().is_err() {
        ui::alert("定位配置文件失败", "无法调用 /usr/bin/open。");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn goQuit() {
    hook::stop();
    std::process::exit(0);
}

#[unsafe(no_mangle)]
pub extern "C" fn goAbout() {
    ui::about(
        "关于 keyboard-plus",
        &format!(
            "keyboard-plus v{APP_VERSION}
键盘防抖工具 · MIT License

作者：{}
GitHub：github.com/singcl
个人网站：imcoco.top",
            crate::AUTHOR
        ),
        crate::PROJECT_URL,
        crate::AUTHOR_SITE_URL,
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn goIconStyle() -> i32 {
    CFG.lock().unwrap().as_ref().map_or(0, |c| c.icon_style as i32)
}

#[unsafe(no_mangle)]
pub extern "C" fn goSetIconStyle(style: i32) {
    set_cfg(|c| c.icon_style = if style == 1 { 1 } else { 0 });
}

#[unsafe(no_mangle)]
pub extern "C" fn goCheckUpdate() {
    check_update(true);
}

// ---------- 更新检查 ----------

fn check_update(interactive: bool) {
    std::thread::spawn(move || match update::latest() {
        Err(e) => {
            if interactive {
                ui::update_alert("检查更新失败", &e, "");
            }
        }
        Ok(rel) => {
            if !update::newer(APP_VERSION, &rel.tag) {
                if interactive {
                    let msg = if update::is_version(APP_VERSION) {
                        format!("当前已是最新版本 v{APP_VERSION}。")
                    } else {
                        "当前已是最新版本。".to_string()
                    };
                    ui::update_alert("检查更新", &msg, "");
                }
                return;
            }

            ui::update_alert(
                "发现新版本",
                &format!("当前版本 v{APP_VERSION}，最新版本 {}。", rel.tag),
                &rel.url,
            );
        }
    });
}

fn bool_to_i32(v: bool) -> i32 {
    i32::from(v)
}
