// 托盘菜单：构建、刷新勾选状态、目标键盘子菜单重建。
//
// 菜单命令 id 固定分配（与 Go 版 walk Action 一一对应），目标键盘条目动态编号。

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DeleteMenu, MENU_ITEM_FLAGS, HMENU,
};

use crate::config::Config;
use crate::devices::{self, Device};

pub mod ids {
    pub const STATUS: u16 = 100;
    pub const PRESET_BASE: u16 = 101; // 101..=104 -> 100/80/67/50ms
    pub const CUSTOM: u16 = 105;
    pub const PAUSE: u16 = 110;
    pub const AUTOSTART: u16 = 111;
    pub const DEVICES: u16 = 112;
    pub const GATE: u16 = 113;
    pub const TARGET_BASE: u16 = 200; // 动态：目标键盘条目，每次重建重新编号
    pub const LOG: u16 = 120;
    pub const CFG_LOC: u16 = 121;
    pub const CLEAR: u16 = 122;
    pub const CHECK_UPDATE: u16 = 123;
    pub const VERSION: u16 = 124;
    pub const EXIT: u16 = 125;
}

pub const PRESETS: [i32; 4] = [100, 80, 67, 50];

pub const FLAG_STRING: MENU_ITEM_FLAGS = MF_STRING;
pub const FLAG_DISABLED: MENU_ITEM_FLAGS = MENU_ITEM_FLAGS(MF_STRING.0 | MF_GRAYED.0 | MF_DISABLED.0);

use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CheckMenuItem, MF_BYCOMMAND, MF_BYPOSITION, MF_CHECKED, MF_DISABLED, MF_GRAYED,
    MF_POPUP, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, ModifyMenuW,
};

/// 目标键盘条目：id -> (设备, 展示标签)，由 rebuild_target 填充。
pub struct TargetItem {
    pub id: u16,
    pub device: Device,
    pub label: String,
}

pub fn new_popup() -> HMENU {
    // SAFETY: 无参调用。
    unsafe { CreatePopupMenu().unwrap_or(HMENU(std::ptr::null_mut())) }
}

pub fn item(menu: HMENU, id: u16, text: &str, flags: MENU_ITEM_FLAGS) {
    let t = super::util::wide(text);
    // SAFETY: 菜单句柄与宽字符串均在调用期间有效。
    unsafe {
        let _ = AppendMenuW(menu, flags, id as usize, super::util::pcw(&t));
    }
}

pub fn separator(menu: HMENU) {
    // SAFETY: 菜单句柄有效。
    unsafe {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, windows::core::PCWSTR::null());
    }
}

pub fn popup(menu: HMENU, submenu: HMENU, text: &str) {
    let t = super::util::wide(text);
    // SAFETY: 菜单句柄与宽字符串均在调用期间有效。
    unsafe {
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(MF_STRING.0 | MF_POPUP.0),
            submenu.0 as usize,
            super::util::pcw(&t),
        );
    }
}

/// 清空并重建「目标键盘」子菜单，刷新条目映射。
pub fn rebuild_target(target: HMENU, items: &mut Vec<TargetItem>, cfg: &Config) {
    // SAFETY: 菜单句柄有效；按位置删除直到空。
    unsafe {
        while DeleteMenu(target, 0, MF_BYPOSITION).is_ok() {}
    }
    items.clear();

    let Ok(list) = devices::list() else {
        item(target, ids::TARGET_BASE, "未检测到键盘设备", FLAG_DISABLED);
        return;
    };
    if list.is_empty() {
        item(target, ids::TARGET_BASE, "未检测到键盘设备", FLAG_DISABLED);
        return;
    }

    let (physical, system) = devices::group(list);
    let physical_labels = devices::labels(&physical);

    let mut next_id = ids::TARGET_BASE;
    for (i, d) in physical.iter().enumerate() {
        add_target_item(target, items, d, physical_labels[i].clone(), &mut next_id, cfg);
    }
    if physical.is_empty() {
        item(target, next_id, "未检测到实体键盘", FLAG_DISABLED);
        next_id += 1;
    }
    if !system.is_empty() {
        separator(target);
        item(target, next_id, "系统 / 虚拟设备", FLAG_DISABLED);
        next_id += 1;
        let system_labels = devices::labels(&system);
        for (i, d) in system.iter().enumerate() {
            add_target_item(target, items, d, system_labels[i].clone(), &mut next_id, cfg);
        }
    }
}

fn add_target_item(
    target: HMENU,
    items: &mut Vec<TargetItem>,
    d: &Device,
    label: String,
    next_id: &mut u16,
    cfg: &Config,
) {
    let checked = d.matches(&cfg.target_name, &cfg.target_path);
    item(
        target,
        *next_id,
        &label,
        MENU_ITEM_FLAGS(MF_STRING.0 | if checked { MF_CHECKED.0 } else { MF_UNCHECKED.0 }),
    );
    items.push(TargetItem { id: *next_id, device: d.clone(), label });
    *next_id += 1;
}

/// 刷新与配置相关的勾选状态和文字（Go 版 syncMenu）。
pub fn sync_config_menus(root: HMENU, debounce: HMENU, hwnd: HWND, cfg: &Config) {
    let _ = hwnd;
    // SAFETY: 菜单与窗口句柄均有效。
    unsafe {
        let mut custom = true;
        for (i, &ms) in PRESETS.iter().enumerate() {
            let id = ids::PRESET_BASE + i as u16;
            let on = ms == cfg.debounce_ms;
            let _ = CheckMenuItem(
                debounce,
                id as u32,
                MF_BYCOMMAND.0 | if on { MF_CHECKED.0 } else { MF_UNCHECKED.0 },
            );
            if on {
                custom = false;
            }
        }
        // 自定义阈值时直接显示当前值，不用打开输入框才能看到
        let custom_text = if custom {
            format!("自定义阈值（{}ms）…", cfg.debounce_ms)
        } else {
            "自定义阈值…".to_string()
        };
        let t = super::util::wide(&custom_text);
        let _ = ModifyMenuW(
            debounce,
            ids::CUSTOM as u32,
            MENU_ITEM_FLAGS(MF_BYCOMMAND.0 | MF_STRING.0),
            ids::CUSTOM as usize,
            super::util::pcw(&t),
        );

        // 子菜单标题带上当前阈值，收起时也能看到
        let title = super::util::wide(&format!("防抖阈值（{}ms）", cfg.debounce_ms));
        let _ = ModifyMenuW(
            root,
            debounce.0 as u32,
            MENU_ITEM_FLAGS(MF_BYCOMMAND.0 | MF_POPUP.0),
            debounce.0 as usize,
            super::util::pcw(&title),
        );

        // 暂停 / 自启 / 仅在线防抖勾选
        let _ = CheckMenuItem(
            root,
            ids::PAUSE as u32,
            MF_BYCOMMAND.0 | if cfg.paused { MF_CHECKED.0 } else { MF_UNCHECKED.0 },
        );
        let _ = CheckMenuItem(
            root,
            ids::AUTOSTART as u32,
            MF_BYCOMMAND.0 | if crate::autostart::enabled() { MF_CHECKED.0 } else { MF_UNCHECKED.0 },
        );
        let _ = CheckMenuItem(
            root,
            ids::GATE as u32,
            MF_BYCOMMAND.0 | if cfg.only_when_connected { MF_CHECKED.0 } else { MF_UNCHECKED.0 },
        );
    }
}

/// 更新状态行文字（Go 版 updateTip 的菜单部分）。
pub fn set_status_text(root: HMENU, text: &str) {
    let t = super::util::wide(text);
    // SAFETY: 菜单句柄与宽字符串均有效。
    unsafe {
        let _ = ModifyMenuW(
            root,
            ids::STATUS as u32,
            MENU_ITEM_FLAGS(MF_BYCOMMAND.0 | MF_STRING.0),
            ids::STATUS as usize,
            super::util::pcw(&t),
        );
    }
}
