// 按平台选择 hook 的实现，对外只暴露 setup / stop。

#[cfg(windows)]
pub use super::windows::{setup, stop};

#[cfg(target_os = "macos")]
pub use super::macos::{setup, stop};

#[cfg(not(any(windows, target_os = "macos")))]
pub use super::stub::{setup, stop};
