// Cocoa 界面的 FFI 声明：cocoa_darwin.m 实现这些入口，Rust 在主线程依次调用。

use std::ffi::CString;

// SAFETY 注释统一放在调用处；原型与 cocoa_darwin.m 一一对应。
unsafe extern "C" {
    fn uiInit(
        normal_png: *const u8,
        normal_len: i32,
        paused_png: *const u8,
        paused_len: i32,
        normal_alt_png: *const u8,
        normal_alt_len: i32,
        paused_alt_png: *const u8,
        paused_alt_len: i32,
        version: *const i8,
        update_enabled: i32,
    );
    fn uiRun();
    fn uiRefresh();
    fn uiAlert(title: *const i8, message: *const i8);
    fn uiUpdateAlert(title: *const i8, message: *const i8, url: *const i8);
    fn uiPermissionAlert();
    fn uiAboutAlert(
        title: *const i8,
        message: *const i8,
        project_url: *const i8,
        site_url: *const i8,
    );
}

pub(crate) fn init(
    normal: &[u8],
    paused: &[u8],
    normal_alt: Option<&[u8]>,
    paused_alt: Option<&[u8]>,
    version: &str,
    update_enabled: bool,
) {
    let v = cstr(version);
    // 备用图为 None（精简版）时传空指针，Objective-C 侧据此不建「图标样式」菜单
    let (na, na_len) = normal_alt.map_or((std::ptr::null(), 0), |b| (b.as_ptr(), b.len()));
    let (pa, pa_len) = paused_alt.map_or((std::ptr::null(), 0), |b| (b.as_ptr(), b.len()));
    // SAFETY: 字符串与字节缓冲区均在调用期间有效；uiInit 内部拷贝图片数据。
    unsafe {
        uiInit(
            normal.as_ptr(),
            normal.len() as i32,
            paused.as_ptr(),
            paused.len() as i32,
            na,
            na_len as i32,
            pa,
            pa_len as i32,
            v.as_ptr() as *const i8,
            i32::from(update_enabled),
        );
    }
}

pub(crate) fn run() {
    // SAFETY: 无参调用，进入 Cocoa 事件循环后不再返回。
    unsafe { uiRun() }
}

pub(crate) fn about(title: &str, message: &str, project_url: &str, site_url: &str) {
    let t = cstr(title);
    let m = cstr(message);
    let u1 = cstr(project_url);
    let u2 = cstr(site_url);
    // SAFETY: 字符串在调用期间有效。
    unsafe {
        uiAboutAlert(t.as_ptr() as *const i8, m.as_ptr() as *const i8, u1.as_ptr() as *const i8, u2.as_ptr() as *const i8);
    }
}

pub(crate) fn refresh() {
    // SAFETY: 无参调用；由事件回调线程（主线程）触发。
    unsafe { uiRefresh() }
}

pub(crate) fn alert(title: &str, message: &str) {
    let t = cstr(title);
    let m = cstr(message);
    // SAFETY: C 字符串在调用期间有效。
    unsafe { uiAlert(t.as_ptr() as *const i8, m.as_ptr() as *const i8) }
}

pub(crate) fn update_alert(title: &str, message: &str, url: &str) {
    let t = cstr(title);
    let m = cstr(message);
    let u = cstr(url);
    // SAFETY: C 字符串在调用期间有效；uiUpdateAlert 内部切换回主线程弹窗。
    unsafe {
        uiUpdateAlert(t.as_ptr() as *const i8, m.as_ptr() as *const i8, u.as_ptr() as *const i8)
    }
}

pub(crate) fn permission_alert() {
    // SAFETY: 无参调用。
    unsafe { uiPermissionAlert() }
}

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap_or_default()
}
