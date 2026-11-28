// 按平台选择 devices 的实现，对外只暴露 list()。

#[cfg(windows)]
pub use super::windows::list;

#[cfg(not(windows))]
pub use super::stub::list;
