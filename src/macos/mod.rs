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

    let gray = gray_png(assets::ICON_PNG);
    ui::init(
        assets::ICON_PNG,
        &gray,
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
#[no_mangle]
pub extern "C" fn goState() -> i32 {
    if TAP_ACTIVE.load(Ordering::Relaxed) == 0 {
        return 2;
    }
    goPaused()
}

#[no_mangle]
pub extern "C" fn goPaused() -> i32 {
    USER_PAUSED.load(Ordering::Relaxed)
}

#[no_mangle]
pub extern "C" fn goBlockedToday() -> i32 {
    stats::today_count().min(i32::MAX as i64) as i32
}

#[no_mangle]
pub extern "C" fn goDebounceMS() -> i32 {
    cfg().debounce_ms
}

#[no_mangle]
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

#[no_mangle]
pub extern "C" fn goSetDebounce(ms: i32) {
    if !(10..=1000).contains(&ms) {
        ui::alert("数值无效", "请输入 10-1000 之间的整数。");
        return;
    }
    set_cfg(|c| c.debounce_ms = ms);
    hook::set_debounce(Duration::from_millis(ms.max(0) as u64));
    logging::event(&format!("防抖阈值调整为 {ms}ms"));
}

#[no_mangle]
pub extern "C" fn goAutostartEnabled() -> i32 {
    bool_to_i32(autostart::enabled())
}

#[no_mangle]
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

#[no_mangle]
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

#[no_mangle]
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

#[no_mangle]
pub extern "C" fn goQuit() {
    hook::stop();
    std::process::exit(0);
}

#[no_mangle]
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

// 把彩色图标转成灰度 PNG，作为暂停状态的图标。
fn gray_png(src: &[u8]) -> Vec<u8> {
    // 解码到 RGBA8 的细节（调色板/tRNS 展开）见 assets::decode_rgba8
    let Ok((w, h, mut buf)) = crate::assets::decode_rgba8(src) else {
        return src.to_vec();
    };

    // 灰度换算与 Go 版 image.Gray 同一加权（0.299R + 0.587G + 0.114B）。
    // Go 公式的 >>24 针对 16 位色值输入；这里输入是 8 位，除数相应为 >>16。
    for px in buf.chunks_exact_mut(4) {
        let (r, g, b) = (px[0] as u32, px[1] as u32, px[2] as u32);
        let l = ((19595 * r + 38470 * g + 7471 * b + (1 << 15)) >> 16) as u8;
        px[0] = l;
        px[1] = l;
        px[2] = l;
    }

    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let Ok(mut writer) = encoder.write_header() else {
        return src.to_vec();
    };
    if writer.write_image_data(&buf).is_err() {
        return src.to_vec();
    }
    out
}
