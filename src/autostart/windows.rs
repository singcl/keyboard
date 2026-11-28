// Windows 实现：通过 HKCU 的 Run 键管理开机自启。

#[cfg(windows)]

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
    RRF_RT_REG_SZ,
};

const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "keyboard-plus";

/// 判断当前 exe 是否已设置为开机自启。
pub fn enabled() -> bool {
    match exe_value() {
        Some(stored) => {
            if stored.is_empty() {
                return false;
            }
            match quoted_exe_path() {
                Ok(exe) => stored.eq_ignore_ascii_case(&exe),
                // 无法定位 exe 时保持 Go 版行为：有值即视为已开启
                Err(_) => true,
            }
        }
        None => false,
    }
}

/// 开启或关闭开机自启。
pub fn set(enabled: bool) -> io::Result<()> {
    let subkey = wide(RUN_KEY_PATH);
    let name = wide(VALUE_NAME);

    let r = if !enabled {
        // SAFETY: 宽字符串在调用期间有效；值不存在也视为成功（与 Go 版一致）。
        unsafe {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), PCWSTR(name.as_ptr()))
        }
    } else {
        match quoted_exe_path() {
            Ok(exe) => {
                let data = wide(&exe);
                let bytes = (data.len() * 2) as u32; // wide() 已含结尾 null
                // SAFETY: 数据缓冲区按字节数传入，在调用期间有效。
                unsafe {
                    RegSetKeyValueW(
                        HKEY_CURRENT_USER,
                        PCWSTR(subkey.as_ptr()),
                        PCWSTR(name.as_ptr()),
                        REG_SZ.0,
                        Some(data.as_ptr().cast()),
                        bytes,
                    )
                }
            }
            Err(e) => return Err(e),
        }
    };

    match r {
        ERROR_SUCCESS => Ok(()),
        e if e == ERROR_FILE_NOT_FOUND_OR_VALUE && !enabled => Ok(()),
        e => Err(reg_error(e)),
    }
}

// 注册表「键或值不存在」的 Win32 错误码（ERROR_FILE_NOT_FOUND = 2），
// Go 版的 registry.ErrNotExist 对应这里。
const ERROR_FILE_NOT_FOUND_OR_VALUE: WIN32_ERROR = WIN32_ERROR(2);

// 读出 Run 键里记录的命令行；键或值不存在时返回 None。
fn exe_value() -> Option<String> {
    let subkey = wide(RUN_KEY_PATH);
    let name = wide(VALUE_NAME);

    // 两段式：先取长度再取内容
    let mut size = 0u32;
    // SAFETY: 只读查询长度，pvdata 为 None。
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if r != ERROR_SUCCESS || size == 0 {
        return None;
    }

    let mut buf = vec![0u16; (size as usize) / 2];
    // SAFETY: 缓冲区按字节数传入。
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if r != ERROR_SUCCESS {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

fn quoted_exe_path() -> io::Result<String> {
    let p = std::env::current_exe()?;
    Ok(format!("\"{}\"", p.display()))
}

fn reg_error(e: WIN32_ERROR) -> io::Error {
    io::Error::from_raw_os_error(e.0 as i32)
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 往返验证：set(true) 后 enabled 必须为 true；结束时恢复用户原有的注册表值。
    #[test]
    fn set_enabled_roundtrip_restores_previous() {
        let previous = exe_value();

        set(true).expect("set(true) 应成功");
        let exe = quoted_exe_path().unwrap();
        let stored = exe_value();
        assert_eq!(stored.as_deref(), Some(exe.as_str()), "读回应与写入一致");
        assert!(enabled(), "写入当前 exe 路径后 enabled() 应为 true");

        set(false).expect("set(false) 应成功");
        assert!(!enabled(), "删除值后 enabled() 应为 false");

        // 恢复用户原有的注册表值（可能是旧版 Go 程序的路径）
        if let Some(old) = previous {
            let subkey = wide(RUN_KEY_PATH);
            let name = wide(VALUE_NAME);
            let data = wide(&old);
            let bytes = (data.len() * 2) as u32;
            // SAFETY: 与 set(true) 相同的写法。
            let r = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    PCWSTR(subkey.as_ptr()),
                    PCWSTR(name.as_ptr()),
                    REG_SZ.0,
                    Some(data.as_ptr().cast()),
                    bytes,
                )
            };
            assert_eq!(r, ERROR_SUCCESS, "恢复原有注册表值失败");
        }
    }
}
