// 按平台选择按键名解析实现。

#[cfg(windows)]
#[path = "keyname_windows.rs"]
mod windows;
#[cfg(target_os = "macos")]
#[path = "keyname_macos.rs"]
mod macos;
#[cfg(not(any(windows, target_os = "macos")))]
#[path = "keyname_stub.rs"]
mod stub;

#[cfg(windows)]
pub(crate) use windows::key_name;
#[cfg(target_os = "macos")]
pub(crate) use macos::key_name;
#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) use stub::key_name;
