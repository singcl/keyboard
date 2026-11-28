// keyboard-plus：键盘防抖。
//
// Windows 入口：托盘图标 + 菜单（见 tray）。
// macOS 入口：菜单栏图标 + CGEventTap（见 macos）。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets;
mod autostart;
mod config;
mod devices;
mod hook;
mod logging;
mod singleinstance;
mod stats;
mod timeutil;
mod update;

#[cfg(windows)]
mod tray;

// 程序版本：发布构建由 build.rs 注入（CI 的 KEYBOARD_PLUS_VERSION，否则 Cargo.toml 版本）。
pub const APP_VERSION: &str = env!("APP_VERSION");

// 作者与项目链接：「关于」对话框（tray::dialog::show_about / macos goAbout）共用。
pub const AUTHOR: &str = "singcl";
pub const PROJECT_URL: &str = "https://github.com/singcl/keyboard";
pub const AUTHOR_SITE_URL: &str = "https://imcoco.top";

#[cfg(windows)]
fn main() {
    let (_guard, ok) = singleinstance::acquire("keyboard-plus");
    if !ok {
        // 已有实例在运行，提示用户从托盘操作，避免双击后「毫无反应」
        tray::util::msg_box_info(
            None,
            "键盘防抖",
            "程序已在运行，请在任务栏右下角的托盘图标中操作。",
        );
        return;
    }

    // fatal 在 windowsgui 模式下没有控制台，用弹窗展示启动错误
    if let Err(e) = tray::run() {
        tray::util::msg_box_error(None, "keyboard-plus", &e);
        std::process::exit(1);
    }
}

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
fn main() {
    macos::main_macos();
}

#[cfg(not(any(windows, target_os = "macos")))]
fn main() {
    eprintln!("keyboard-plus：当前平台暂不支持界面");
}
