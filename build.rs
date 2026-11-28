// 构建脚本：
// - 所有平台：把版本号（CI 的 KEYBOARD_PLUS_VERSION，否则 Cargo.toml 的版本）以
//   APP_VERSION 常量注入程序，托盘菜单与更新检查共用。
// - Windows：用版本号生成 RC 资源（图标、清单、双语言版本信息）后经 embed-resource 编译。
// - macOS：把 native/macos 下的 Objective-C / C 源文件交给 cc 编译（沿用 Go 版的 cgo 源文件）。

#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::path::PathBuf;
use std::env;

fn main() {
    let version = version();
    println!("cargo:rustc-env=APP_VERSION={version}");

    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => build_windows(&version),
        Ok("macos") => build_macos(),
        _ => {}
    }
}

// 发布构建时 CI 会设置 KEYBOARD_PLUS_VERSION（形如 1.2.6），本地构建退回 Cargo.toml 的版本。
fn version() -> String {
    env::var("KEYBOARD_PLUS_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").unwrap())
}

// build.rs 本身按宿主机编译：embed_resource 只声明为 Windows 宿主的
// build-dependency，因此这段必须 cfg(windows) 排除，否则在 macOS/Linux
// 宿主上连 build 脚本都编不过（embed_resource 不在作用域）。
#[cfg(windows)]
fn build_windows(version: &str) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let rc_path = out_dir.join("keyboard-plus.rc");
    fs::write(&rc_path, windows_rc(version)).unwrap();
    println!("cargo:rerun-if-changed=build.rs");

    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}

#[cfg(not(windows))]
fn build_windows(_version: &str) {}

// VERSIONINFO 与 go 版 winres.json 保持一致：英文与简体中文两套字符串。
#[cfg(windows)]
fn windows_rc(version: &str) -> String {
    let mut v = version.to_string();
    while v.matches('.').count() < 3 {
        v.push_str(".0");
    }
    let nums = v.replace('.', ",");

    format!(
        r##"#pragma code_page(65001)

1 ICON "assets/icon.ico"

1 24 "assets/keyboard.manifest"

1 VERSIONINFO
FILEVERSION {nums}
PRODUCTVERSION {nums}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "singcl"
            VALUE "FileDescription", "Keyboard debounce utility"
            VALUE "FileVersion", "{v}.0"
            VALUE "InternalName", "keyboard-plus"
            VALUE "OriginalFilename", "keyboard-plus.exe"
            VALUE "ProductName", "keyboard-plus"
            VALUE "ProductVersion", "{v}.0"
        END
        BLOCK "080404b0"
        BEGIN
            VALUE "CompanyName", "singcl"
            VALUE "FileDescription", "键盘防误触工具"
            VALUE "FileVersion", "{v}.0"
            VALUE "InternalName", "keyboard-plus"
            VALUE "OriginalFilename", "keyboard-plus.exe"
            VALUE "ProductName", "键盘防抖"
            VALUE "ProductVersion", "{v}.0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200, 0x0804, 1200
    END
END
"##
    )
}

// macOS 的界面与事件监听沿用 Go 版的 Objective-C / C 文件（native/macos/），
// 仅把 cgo 的 _cgo_export.h 换成 bridge.h（由 Rust 侧 #[no_mangle] 导出同一批函数）。
fn build_macos() {
    let mut build = cc::Build::new();
    build
        .file("native/macos/cocoa_darwin.m")
        .file("native/macos/hook_darwin.c")
        .include("native/macos")
        .flag_if_supported("-fobjc-arc")
        .compile("keyboard_mac");

    println!("cargo:rerun-if-changed=native/macos/cocoa_darwin.m");
    println!("cargo:rerun-if-changed=native/macos/hook_darwin.c");
    println!("cargo:rerun-if-changed=native/macos/bridge.h");

    for framework in ["Cocoa", "CoreGraphics", "CoreFoundation", "ApplicationServices"] {
        println!("cargo:rustc-link-arg-bins=-Wl,-framework,{framework}");
    }
}
