// Windows 界面：托盘图标 + 右键菜单 + 阈值对话框 + 日志窗口。
//
// 结构与 Go 版 main.go 一致：UI 线程跑消息循环；钩子线程、看门狗线程、更新检查
// 通过 PostMessage 与 UI 通信；配置由全局互斥锁保护，UI 与看门狗线程都可读。

mod dialog;
mod icon;
mod logview;
pub(crate) mod menu;
pub(crate) mod util;

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetSysColorBrush, COLOR_WINDOW, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NIIF_INFO, NOTIFY_ICON_DATA_FLAGS, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetCursorPos, LoadCursorW,
    PostMessageW, PostQuitMessage, RegisterClassExW, SetForegroundWindow, TrackPopupMenu,
    DestroyWindow, CS_DBLCLKS, MSG, SHOW_WINDOW_CMD, SW_SHOWNORMAL, TPM_BOTTOMALIGN,
    TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TRACK_POPUP_MENU_FLAGS,
    WNDCLASSEXW, WM_APP, WM_CLOSE, WM_DESTROY, WM_NULL, WS_OVERLAPPED,
};

use crate::assets;
use crate::config::{self, Config};
use crate::devices;
use crate::hook;
use crate::logging;
use crate::stats;
use crate::update;
use crate::APP_VERSION;

// UI 与其他线程的通信消息
const WM_APP_TRAY: u32 = WM_APP; // 托盘回调
const WM_APP_REFRESH: u32 = WM_APP + 1;
const WM_APP_TARGET: u32 = WM_APP + 2;
const WM_APP_UPDATE: u32 = WM_APP + 3;

// 目标键盘是否在线：看门狗线程与 UI 线程共享
static TARGET_ONLINE: AtomicBool = AtomicBool::new(true);
// 主窗口句柄（供其他线程 PostMessage）
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
// 配置：UI 写、看门狗读
static CFG: Mutex<Option<Config>> = Mutex::new(None);
// 更新检查结果邮箱：检查线程写入，UI 取走
static UPDATE_MAILBOX: Mutex<Option<Result<update::Release, String>>> = Mutex::new(None);

fn cfg() -> Config {
    CFG.lock().unwrap().clone().unwrap_or_default()
}

fn update_cfg(f: impl FnOnce(&mut Config)) {
    let mut guard = CFG.lock().unwrap();
    if let Some(c) = guard.as_mut() {
        f(c);
        let _ = config::save(c);
    }
}

fn post_main(msg: u32, wparam: WPARAM, lparam: LPARAM) {
    let hwnd = MAIN_HWND.load(Ordering::Relaxed);
    if hwnd != 0 {
        // SAFETY: 句柄在消息循环退出前有效。
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), msg, wparam, lparam);
        }
    }
}

/// 托盘程序入口：创建窗口与托盘图标、拉起后台线程、进入消息循环。
/// 返回 Err 时的展示由调用方处理（windowsgui 下没有控制台）。
pub fn run() -> Result<(), String> {
    let cfg0 = config::load();
    *CFG.lock().unwrap() = Some(cfg0.clone());

    // 配置了「仅目标键盘在线时防抖」时，启动先探测一次在线状态
    if cfg0.only_when_connected && target_configured(&cfg0) {
        let online = devices::online(&cfg0.target_name, &cfg0.target_path);
        TARGET_ONLINE.store(online, Ordering::Relaxed);
        if !online {
            logging::event("启动时目标键盘未连接，暂不拦截");
        }
    }

    let hwnd = create_main_window()?;
    MAIN_HWND.store(hwnd.0 as isize, Ordering::Relaxed);

    let icons = icon::load(assets::ICON_PNG, assets::ICON_PAUSED_PNG)?;

    // 托盘图标（NOTIFYICONDATAW 含裸指针，只存放在 UI 线程的 thread_local 里）
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NOTIFY_ICON_DATA_FLAGS(NIF_MESSAGE.0 | NIF_ICON.0 | NIF_TIP.0);
    nid.uCallbackMessage = WM_APP_TRAY;
    nid.hIcon = icons.pick(cfg0.icon_style).0;
    nid.szTip = fixed_bytes::<128>("键盘防抖");
    // SAFETY: nid 字段已完整填写。
    unsafe {
        if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            return Err("创建托盘图标失败".into());
        }
    }
    NID_SLOT.with(|slot| *slot.borrow_mut() = Some(nid));
    APP_SLOT.with(|slot| *slot.borrow_mut() = Some(App { hwnd, icons, menus: None }));

    build_menus();
    update_tip();

    hook::set_debounce(Duration::from_millis(cfg().debounce_ms.max(0) as u64));
    apply_pause_state();

    // 首次运行气泡提示
    show_balloon("右键图标设置", "右键图标可设置防抖阈值、目标键盘或退出");

    // 启动时静默检查一次更新，只有存在新版本才会提示
    if update::enabled() {
        check_update(false);
    }

    // 统计今日拦截次数，同时写入拦截日志；拦截发生时刷新状态行
    let handler: hook::BlockHandler = std::sync::Arc::new(|vk, scan, ext, interval| {
        stats::add_blocked();
        logging::blocked(vk, scan, ext, interval);
        post_main(WM_APP_REFRESH, WPARAM(0), LPARAM(0));
    });
    std::thread::Builder::new()
        .name("keyboard-hook".into())
        .spawn(move || {
            if let Err(e) = hook::setup(handler) {
                logging::event(&e);
            }
        })
        .map_err(|e| format!("启动钩子线程失败：{e}"))?;

    // 看门狗：定时检测目标键盘是否在线，自动暂停或恢复拦截
    std::thread::Builder::new()
        .name("target-watch".into())
        .spawn(|| loop {
            std::thread::sleep(Duration::from_secs(2));
            post_main(WM_APP_REFRESH, WPARAM(0), LPARAM(0));

            let c = cfg();
            if !c.only_when_connected || !target_configured(&c) {
                continue;
            }
            let online = devices::online(&c.target_name, &c.target_path);
            if online != TARGET_ONLINE.load(Ordering::Relaxed) {
                TARGET_ONLINE.store(online, Ordering::Relaxed);
                post_main(WM_APP_TARGET, WPARAM(online as usize), LPARAM(0));
            }
        })
        .map_err(|e| format!("启动看门狗线程失败：{e}"))?;

    // 消息循环
    let mut m = MSG::default();
    // SAFETY: 常规消息循环；GetMessage 返回 0 表示收到 WM_QUIT。
    unsafe {
        while GetMessageW(&mut m, None, 0, 0).0 > 0 {
            DispatchMessageW(&m);
        }

        // 退出时移除托盘图标与窗口
        NID_SLOT.with(|slot| {
            if let Some(nid) = slot.borrow().as_ref() {
                let _ = Shell_NotifyIconW(NIM_DELETE, nid);
            }
        });
        let _ = DestroyWindow(hwnd);
    }
    Ok(())
}

struct App {
    hwnd: HWND,
    icons: icon::Icons,
    menus: Option<Menus>,
}

// 根菜单、阈值与目标键盘子菜单的句柄集合（HMENU 为裸指针包装，仅 UI 线程使用）
struct Menus {
    root: windows::Win32::UI::WindowsAndMessaging::HMENU,
    debounce: windows::Win32::UI::WindowsAndMessaging::HMENU,
    target: windows::Win32::UI::WindowsAndMessaging::HMENU,
    /// 「图标样式」子菜单；精简版无此子菜单，为空句柄
    iconstyle: windows::Win32::UI::WindowsAndMessaging::HMENU,
    /// 目标键盘条目映射（id -> 设备），右键重建时刷新
    target_items: Vec<menu::TargetItem>,
}

// App 与托盘数据只在 UI 线程上访问，用 thread_local 保存
thread_local! {
    static APP_SLOT: RefCell<Option<App>> = const { RefCell::new(None) };
    static NID_SLOT: RefCell<Option<NOTIFYICONDATAW>> = const { RefCell::new(None) };
}

fn with_app(f: impl FnOnce(&mut App)) {
    APP_SLOT.with(|slot| {
        if let Some(app) = slot.borrow_mut().as_mut() {
            f(app);
        }
    });
}

// 从 App 上取数据：借用随闭包结束立即释放，绝不跨模态循环（TrackPopupMenu /
// MessageBox / 阈值对话框都会在内部泵消息，期间 WM_APP_* 处理器还要再进 with_app）。
fn with_app_ret<T>(f: impl FnOnce(&mut App) -> T) -> Option<T> {
    APP_SLOT.with(|slot| slot.borrow_mut().as_mut().map(f))
}

fn with_nid(f: impl FnOnce(&mut NOTIFYICONDATAW)) {
    NID_SLOT.with(|slot| {
        if let Some(nid) = slot.borrow_mut().as_mut() {
            f(nid);
        }
    });
}

fn target_configured(c: &Config) -> bool {
    !c.target_name.is_empty() || !c.target_path.is_empty()
}

fn create_main_window() -> Result<HWND, String> {
    // SAFETY: 窗口类与实例句柄均有效；类名以 null 结尾。
    unsafe {
        let class_name = util::wide("keyboard-plus-tray");
        let hinstance = GetModuleHandleW(None).map_err(|e| format!("取模块句柄失败：{e}"))?;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(tray_wnd_proc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, windows::Win32::UI::WindowsAndMessaging::IDC_ARROW)
                .map_err(|e| format!("加载光标失败：{e}"))?,
            hbrBackground: HBRUSH(GetSysColorBrush(COLOR_WINDOW).0),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            return Err("注册窗口类失败".into());
        }

        CreateWindowExW(
            Default::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(class_name.as_ptr()),
            WS_OVERLAPPED, // 隐藏的消息窗口
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .map_err(|e| format!("创建窗口失败：{e}"))
    }
}

// SAFETY: 系统在 UI 线程调用；App 通过 thread_local 访问。
unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: 系统回调；所有 Win32 调用参数均由系统提供或 UI 线程持有。
    unsafe { match msg {
        WM_APP_TRAY => {
            // 低字是鼠标消息：仅处理右键弹起（与 Go 版 walk MouseUp 一致）
            if (lparam.0 as u32) & 0xFFFF == WM_RBUTTONUP {
                show_context_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_APP_REFRESH => {
            update_tip();
            LRESULT(0)
        }
        WM_APP_TARGET => {
            apply_pause_state();
            if wparam.0 != 0 {
                logging::event("目标键盘已连接，自动恢复拦截");
            } else {
                logging::event("目标键盘已断开，自动暂停拦截");
            }
            LRESULT(0)
        }
        WM_APP_UPDATE => {
            handle_update_result(wparam.0 != 0);
            LRESULT(0)
        }
        WM_CLOSE => {
            // 托盘程序：关闭请求一律忽略（与 Go 版隐藏主窗口一致）
            LRESULT(0)
        }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

const WM_RBUTTONUP: u32 = 0x0205;

fn show_context_menu(hwnd: HWND) {
    // 每次右键打开菜单前刷新设备列表与开机自启状态，保证菜单与实际一致。
    // 借用只在「改菜单」的短促段内持有；TrackPopupMenu 自带消息泵，
    // 期间看门狗的 WM_APP_REFRESH 还会进来更新状态行，不能锁着 App。
    {
        let c = cfg();
        rebuild_target_menu();
        with_app(|app| {
            let menus = app.menus.as_ref().expect("菜单已构建");
            menu::sync_config_menus(menus.root, menus.debounce, menus.iconstyle, hwnd, &c);
        });
    }
    let Some(root) = with_app_ret(|app| app.menus.as_ref().map(|m| m.root)) else {
        return;
    };
    let Some(root) = root else { return };

    // SAFETY: 弹出菜单前先切前台，否则点击菜单外区域不关闭（官方建议）。
    let cmd_id = unsafe {
        let _ = SetForegroundWindow(hwnd);
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let cmd = TrackPopupMenu(
            root,
            TRACK_POPUP_MENU_FLAGS(
                TPM_LEFTALIGN.0
                    | TPM_RIGHTBUTTON.0
                    | TPM_RETURNCMD.0
                    | TPM_NONOTIFY.0
                    | TPM_BOTTOMALIGN.0,
            ),
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        cmd.0 as u16
    };
    if cmd_id != 0 {
        handle_command(hwnd, cmd_id);
    }
}

// 菜单命令分发：在顶层调用（不持有 App 借用）。
// 弹窗、对话框等模态调用必须在借用之外——它们内部会泵消息。
fn handle_command(hwnd: HWND, id: u16) {
    use menu::ids;

    match id {
        _ if (ids::PRESET_BASE..=ids::PRESET_BASE + 3).contains(&id) => {
            let ms = menu::PRESETS[(id - ids::PRESET_BASE) as usize];
            apply_debounce(ms);
        }
        ids::CUSTOM => {
            let current = cfg().debounce_ms;
            if let Some(ms) = dialog::show_threshold(hwnd, current) {
                apply_debounce(ms);
            }
        }
        ids::PAUSE => {
            let now_paused = !cfg().paused;
            update_cfg(|c| c.paused = now_paused);
            apply_pause_state();
            if now_paused {
                logging::event("已手动暂停拦截");
            } else {
                logging::event("已手动恢复拦截");
            }
        }
        ids::AUTOSTART => {
            let enable = !crate::autostart::enabled();
            if let Err(e) = crate::autostart::set(enable) {
                util::msg_box_error(Some(hwnd), "设置开机自启失败", &e.to_string());
                return;
            }
            if enable {
                logging::event("已开启开机自启");
            } else {
                logging::event("已关闭开机自启");
            }
            sync_checkboxes();
        }
        ids::DEVICES => match devices::list() {
            Err(e) => util::msg_box_error(Some(hwnd), "识别键盘设备失败", &e),
            Ok(list) if list.is_empty() => {
                util::msg_box_info(Some(hwnd), "键盘设备", "未检测到键盘设备。")
            }
            Ok(list) => {
                let c = cfg();
                let text = devices::format_list(&list, &c.target_name, &c.target_path);
                util::msg_box_info(Some(hwnd), "键盘设备", &text);
            }
        },
        ids::GATE => {
            let enable = !cfg().only_when_connected;
            let c = cfg();
            if enable && !target_configured(&c) {
                util::msg_box_info(
                    Some(hwnd),
                    "未设置目标键盘",
                    "请先在「目标键盘」子菜单中选择要匹配的设备。",
                );
                return;
            }
            update_cfg(|cfg| cfg.only_when_connected = enable);
            let online = if enable {
                devices::online(&c.target_name, &c.target_path)
            } else {
                true
            };
            TARGET_ONLINE.store(online, Ordering::Relaxed);
            if enable {
                logging::event("已开启仅目标键盘在线时防抖");
            } else {
                logging::event("已关闭仅目标键盘在线时防抖");
            }
            apply_pause_state();
        }
        _ if (ids::TARGET_BASE..ids::TARGET_BASE + 300).contains(&id) => {
            let picked = with_app_ret(|app| {
                app.menus
                    .as_ref()?
                    .target_items
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| (t.device.clone(), t.label.clone()))
            })
            .flatten();
            let Some((device, label)) = picked else { return };
            let gate = cfg().only_when_connected;
            update_cfg(|c| {
                c.target_name = device.name.clone();
                c.target_path = device.path.clone();
            });
            let online = if gate {
                devices::online(&device.name, &device.path)
            } else {
                true
            };
            TARGET_ONLINE.store(online, Ordering::Relaxed);
            apply_pause_state();
            logging::event(&format!("目标键盘切换为：{label}"));
            // 菜单里的勾选标记要移动到新目标上
            rebuild_target_menu();
        }
        ids::LOG => logview::show(hwnd),
        ids::CFG_LOC => match config::path() {
            Err(e) => util::msg_box_error(Some(hwnd), "定位配置文件失败", &e.to_string()),
            Ok(path) => {
                let target = if path.exists() {
                    path
                } else {
                    path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
                };
                let target_w = util::wide(&target.to_string_lossy());
                // SAFETY: 路径字符串在调用期间有效。
                let r = unsafe {
                    ShellExecuteW(
                        None,
                        PCWSTR(w!("open").as_ptr()),
                        util::pcw(&target_w),
                        None,
                        None,
                        SHOW_WINDOW_CMD(SW_SHOWNORMAL.0),
                    )
                };
                if r.0 as usize <= 32 {
                    util::msg_box_error(Some(hwnd), "定位配置文件失败", "无法在资源管理器中打开。");
                }
            }
        },
        ids::CLEAR => {
            if !util::msg_box_question(Some(hwnd), "清空日志", "确定要清空拦截日志吗？") {
                return;
            }
            if let Err(e) = logging::clear() {
                util::msg_box_error(Some(hwnd), "清空日志失败", &e.to_string());
            }
        }
        ids::CHECK_UPDATE => check_update(true),
        ids::EXIT => unsafe {
            PostQuitMessage(0);
        },
        // 图标样式切换（标准版）：0 键盘板 / 1 脉冲屏
        id if (menu::ids::ICON_STYLE_BASE..=menu::ids::ICON_STYLE_BASE + 1).contains(&id) => {
            let style = (id - menu::ids::ICON_STYLE_BASE) as u8;
            update_cfg(|c| c.icon_style = style);
            apply_pause_state();
            logging::event(&format!("图标样式切换为：{}", menu::ICON_STYLES[style as usize]));
        }
        _ => {}
    }
}

fn apply_debounce(ms: i32) {
    update_cfg(|c| c.debounce_ms = ms);
    hook::set_debounce(Duration::from_millis(ms.max(0) as u64));
    sync_checkboxes();
    update_tip();
    logging::event(&format!("防抖阈值调整为 {ms}ms"));
}

// 计算「实际是否暂停」并同步到钩子、图标与状态行（Go 版 applyPauseState + updateTip）。
fn apply_pause_state() {
    let c = cfg();
    let effective = c.paused
        || (c.only_when_connected && target_configured(&c) && !TARGET_ONLINE.load(Ordering::Relaxed));
    hook::set_paused(effective);

    let icon = {
        let mut icon = None;
        with_app(|app| {
            let (normal, paused) = app.icons.pick(c.icon_style);
            icon = Some(if c.paused || effective { paused } else { normal });
        });
        icon
    };
    if let Some(icon) = icon {
        with_nid(|nid| {
            nid.hIcon = icon;
            nid.uFlags = NOTIFY_ICON_DATA_FLAGS(NIF_ICON.0);
            // SAFETY: nid 已填充并在托盘注册。
            unsafe {
                let _ = Shell_NotifyIconW(NIM_MODIFY, nid);
            }
        });
    }
    update_tip();
    sync_checkboxes();
}

fn update_tip() {
    let c = cfg();
    let online = TARGET_ONLINE.load(Ordering::Relaxed);
    let state = if c.paused {
        "已暂停"
    } else if c.only_when_connected && target_configured(&c) && !online {
        "已暂停（目标键盘未连接）"
    } else {
        "运行中"
    };

    let mut text = format!("状态：{state}");
    let n = stats::today_count();
    if n > 0 {
        text += &format!(" · 今日拦截 {n} 次");
    }

    with_app(|app| {
        if let Some(menus) = app.menus.as_ref() {
            menu::set_status_text(menus.root, &text);
        }
    });
    with_nid(|nid| {
        nid.szTip = fixed_bytes::<128>(&format!("键盘防抖 {}ms · {state}", c.debounce_ms));
        nid.uFlags = NOTIFY_ICON_DATA_FLAGS(NIF_TIP.0);
        // SAFETY: nid 已填充并在托盘注册。
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, nid);
        }
    });
}

fn sync_checkboxes() {
    with_app(|app| {
        if let Some(menus) = app.menus.as_ref() {
            let c = cfg();
            menu::sync_config_menus(menus.root, menus.debounce, menus.iconstyle, app.hwnd, &c);
        }
    });
}

fn rebuild_target_menu() {
    with_app(|app| {
        let c = cfg();
        if let Some(menus) = app.menus.as_mut() {
            menu::rebuild_target(menus.target, &mut menus.target_items, &c);
        }
    });
}

fn fixed_bytes<const N: usize>(s: &str) -> [u16; N] {
    let mut arr = [0u16; N];
    for (i, ch) in s.encode_utf16().take(N - 1).enumerate() {
        arr[i] = ch;
    }
    arr
}

fn show_balloon(title: &str, text: &str) {
    with_nid(|nid| {
        nid.szInfo = fixed_bytes::<256>(text);
        nid.szInfoTitle = fixed_bytes::<64>(title);
        nid.dwInfoFlags = NIIF_INFO;
        nid.uFlags = NOTIFY_ICON_DATA_FLAGS(NIF_INFO.0);
        // SAFETY: nid 已填充并在托盘注册。
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, nid);
        }
    });
}

// 检查更新：结果写入邮箱后通知 UI 线程弹窗（interactive=false 时只有新版本才提示）。
fn check_update(interactive: bool) {
    if !update::enabled() {
        return;
    }
    std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            let result = update::latest();
            *UPDATE_MAILBOX.lock().unwrap() = Some(result);
            post_main(WM_APP_UPDATE, WPARAM(interactive as usize), LPARAM(0));
        })
        .ok();
}

fn handle_update_result(interactive: bool) {
    let mailbox = UPDATE_MAILBOX.lock().unwrap().take();
    let Some(result) = mailbox else { return };
    let hwnd = HWND(MAIN_HWND.load(Ordering::Relaxed) as *mut _);

    match result {
        Err(e) => {
            if interactive {
                util::msg_box_error(Some(hwnd), "检查更新失败", &e);
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
                    util::msg_box_info(Some(hwnd), "检查更新", &msg);
                }
                return;
            }

            let msg = format!(
                "当前版本 v{APP_VERSION}，最新版本 {}。\n\n是否打开下载页面？",
                rel.tag
            );
            if util::msg_box_question(Some(hwnd), "发现新版本", &msg) {
                let url = util::wide(&rel.url);
                // SAFETY: URL 字符串在调用期间有效。
                let _ = unsafe {
                    ShellExecuteW(
                        None,
                        PCWSTR(w!("open").as_ptr()),
                        util::pcw(&url),
                        None,
                        None,
                        SHOW_WINDOW_CMD(SW_SHOWNORMAL.0),
                    )
                };
            }
        }
    }
}

// 菜单构建：状态行 → 阈值子菜单 → 命令项 → 目标键盘子菜单 → 日志/配置/更新/退出
fn build_menus() {
    let root = menu::new_popup();
    let debounce = menu::new_popup();
    let target = menu::new_popup();
    let switchable = with_app_ret(|app| app.icons.switchable()).unwrap_or(false);

    menu::item(root, menu::ids::STATUS, "状态：运行中", menu::FLAG_DISABLED);
    menu::separator(root);

    menu::popup(root, debounce, "防抖阈值");
    for (i, ms) in menu::PRESETS.iter().enumerate() {
        menu::item(debounce, menu::ids::PRESET_BASE + i as u16, &format!("{ms}ms"), menu::FLAG_STRING);
    }
    menu::item(debounce, menu::ids::CUSTOM, "自定义阈值…", menu::FLAG_STRING);
    menu::separator(root);

    menu::item(root, menu::ids::PAUSE, "暂停防抖", menu::FLAG_STRING);
    menu::item(root, menu::ids::AUTOSTART, "开机自启", menu::FLAG_STRING);
    menu::separator(root);
    menu::item(root, menu::ids::DEVICES, "识别键盘设备", menu::FLAG_STRING);
    menu::item(root, menu::ids::GATE, "仅目标键盘在线时防抖", menu::FLAG_STRING);
    menu::popup(root, target, "目标键盘");
    // 图标样式切换：精简版（单套图标）不建此子菜单
    let iconstyle = menu::new_popup();
    if switchable {
        menu::popup(root, iconstyle, "图标样式");
        for (i, name) in menu::ICON_STYLES.iter().enumerate() {
            menu::item(iconstyle, menu::ids::ICON_STYLE_BASE + i as u16, name, menu::FLAG_STRING);
        }
    }
    menu::separator(root);
    menu::item(root, menu::ids::LOG, "查看日志", menu::FLAG_STRING);
    menu::item(root, menu::ids::CFG_LOC, "配置文件位置", menu::FLAG_STRING);
    menu::item(root, menu::ids::CLEAR, "清空日志", menu::FLAG_STRING);
    menu::separator(root);
    if update::enabled() {
        menu::item(root, menu::ids::CHECK_UPDATE, "检查更新…", menu::FLAG_STRING);
    }
    menu::item(root, menu::ids::VERSION, &format!("版本 v{APP_VERSION}"), menu::FLAG_DISABLED);
    menu::separator(root);
    menu::item(root, menu::ids::EXIT, "退出", menu::FLAG_STRING);

    let mut items = Vec::new();
    menu::rebuild_target(target, &mut items, &cfg());

    with_app(|app| {
        app.menus = Some(Menus { root, debounce, target, iconstyle, target_items: items });
    });
}
