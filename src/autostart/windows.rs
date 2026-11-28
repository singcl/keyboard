// Windows 实现：通过 HKCU 的 Run 键管理开机自启。

#[cfg(windows)]

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegSetValueW, HKEY,
    HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
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
    let mut hkey = HKEY::default();

    // SAFETY: 输出句柄与宽字符串均在调用期间有效。
    let r = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut hkey,
            None,
        )
    };
    if r != ERROR_SUCCESS {
        return Err(reg_error(r));
    }

    let result = if !enabled {
        let name = wide(VALUE_NAME);
        // SAFETY: 宽字符串在调用期间有效；Go 版把「值不存在」也视为成功。
        let r = unsafe { RegDeleteValueW(hkey, PCWSTR(name.as_ptr())) };
        match r {
            ERROR_SUCCESS => Ok(()),
            e if e == ERROR_FILE_NOT_FOUND_OR_VALUE => Ok(()),
            e => Err(reg_error(e)),
        }
    } else {
        match quoted_exe_path() {
            Ok(exe) => {
                let name = wide(VALUE_NAME);
                let data = wide(&exe);
                let bytes = (data.len() * 2) as u32;
                // SAFETY: 数据缓冲区按字节数传入（wide() 已含结尾 null）。
                let r = unsafe {
                    RegSetValueW(hkey, PCWSTR(name.as_ptr()), REG_SZ, PCWSTR(data.as_ptr()), bytes)
                };
                if r == ERROR_SUCCESS {
                    Ok(())
                } else {
                    Err(reg_error(r))
                }
            }
            Err(e) => Err(e),
        }
    };

    // SAFETY: 句柄由 RegCreateKeyExW 返回。
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    result
}

// 注册表「值不存在」的 Win32 错误码（ERROR_FILE_NOT_FOUND = 2），
// RegDeleteValueW 删除不存在的值时返回它，Go 版的 registry.ErrNotExist 对应这里。
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
