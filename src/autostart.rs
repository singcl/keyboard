// 开机自启，具体实现按平台拆分：Windows 走 HKCU Run 键，macOS 写 LaunchAgent。

#[cfg(windows)]
mod windows;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(windows, target_os = "macos")))]
mod stub;

#[cfg(windows)]
pub use windows::{enabled, set};

#[cfg(target_os = "macos")]
pub use macos::{enabled, set};

#[cfg(not(any(windows, target_os = "macos")))]
pub use stub::{enabled, set};
