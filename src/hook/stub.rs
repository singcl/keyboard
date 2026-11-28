// 其他平台占位实现：暂不支持全局键盘拦截。

#[cfg(not(any(windows, target_os = "macos")))]

use super::BlockHandler;

/// 暂未实现，直接返回。
pub fn setup(handler: BlockHandler) -> Result<(), String> {
    let _ = handler;
    eprintln!("hook: 当前平台暂不支持全局键盘拦截，防抖功能未生效");
    Ok(())
}

pub fn stop() {}
