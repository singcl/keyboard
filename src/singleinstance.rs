// 单实例：保证同一时间只运行一个程序实例。
// Windows 用命名互斥量，macOS 用文件锁，其他平台直接放行。

/// Acquire 成功时返回的释放句柄；drop 时释放锁。
pub struct Guard {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
    #[cfg(target_os = "macos")]
    fd: std::os::fd::RawFd,
    _name: String,
}

impl Drop for Guard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
        #[cfg(target_os = "macos")]
        unsafe {
            let _ = libc::flock(self.fd, libc::LOCK_UN);
            let _ = libc::close(self.fd);
        }
    }
}

/// 尝试获取名为 name 的单实例锁。
/// ok 为 false 表示已有实例在运行；否则应把 Guard 保持到程序退出。
pub fn acquire(name: &str) -> (Option<Guard>, bool) {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;

        let wide: Vec<u16> = std::ffi::OsStr::new(name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // SAFETY: 宽字符串在调用期间有效；无安全属性、不请求初始所有权。
        let handle = unsafe {
            let r = CreateMutexW(None, false, PCWSTR(wide.as_ptr()));
            match r {
                Ok(h) => h,
                // 无法创建互斥量时不阻止启动
                Err(_) => return (None, true),
            }
        };

        // GetLastError 恰好在 CreateMutexW 之后调用，值仍有效。
        if unsafe { windows::Win32::Foundation::GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return (None, false);
        }

        (Some(Guard { handle, _name: name.to_string() }), true)
    }

    #[cfg(target_os = "macos")]
    {
        let dir = match crate::config::base_cache_dir() {
            Ok(d) => d.join("keyboard-plus"),
            Err(_) => return (None, true),
        };
        if std::fs::create_dir_all(&dir).is_err() {
            return (None, true);
        }

        let path = dir.join(format!("{name}.lock"));
        let Ok(cstr) = std::ffi::CString::new(path.to_string_lossy().as_bytes()) else {
            return (None, true);
        };

        // SAFETY: open/flock/fdclose 均为常规 POSIX 调用；O_RDWR|O_CREAT 语义与 Go 版一致。
        unsafe {
            let fd = libc::open(cstr.as_ptr(), libc::O_RDWR | libc::O_CREAT, 0o644);
            if fd < 0 {
                // 无法创建锁文件时不阻止启动
                return (None, true);
            }
            if libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) != 0 {
                libc::close(fd);
                return (None, false);
            }
            (Some(Guard { fd, _name: name.to_string() }), true)
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = name;
        (None, true)
    }
}

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn second_acquire_fails() {
        let name = format!("keyboard-test-mutex-{}", std::process::id());
        let (g1, ok1) = acquire(&name);
        assert!(ok1, "first Acquire should succeed");

        let (_, ok2) = acquire(&name);
        assert!(!ok2, "second Acquire should fail while first is held");

        drop(g1);

        let (g3, ok3) = acquire(&name);
        assert!(ok3, "Acquire should succeed after release");
        drop(g3);
    }
}
