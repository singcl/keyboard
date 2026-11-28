// 单实例：保证同一时间只运行一个程序实例。
// Windows 用命名互斥量，macOS 用 O_EXCL 创建式文件锁，其他平台直接放行。

/// Acquire 成功时返回的释放句柄；drop 时释放锁。
pub struct Guard {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
    #[cfg(target_os = "macos")]
    path: std::path::PathBuf,
    _name: String,
}

impl Drop for Guard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
        #[cfg(target_os = "macos")]
        {
            // 文件的存在即是锁；正常退出时移除。崩溃残留由 PID 探活清理。
            let _ = std::fs::remove_file(&self.path);
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
        use std::io::Write;

        let dir = match crate::config::base_cache_dir() {
            Ok(d) => d.join("keyboard-plus"),
            Err(_) => return (None, true),
        };
        if std::fs::create_dir_all(&dir).is_err() {
            return (None, true);
        }
        let path = dir.join(format!("{name}.lock"));

        // O_EXCL 创建式锁：文件存在即视为已有实例，同进程与跨进程语义一致
        // （flock 在 macOS 上对同进程重复加锁的行为不确定，CI 实测过两种结果）。
        // 崩溃残留的锁文件用 PID 存活探测清理。
        let mut attempts = 0;
        loop {
            attempts += 1;
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut f) => {
                    let _ = writeln!(f, "{}", std::process::id());
                    return (Some(Guard { path, _name: name.to_string() }), true);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if stale_lock(&path) {
                        let _ = std::fs::remove_file(&path);
                        if attempts < 2 {
                            continue; // 清掉崩溃残留后重试一次
                        }
                    }
                    return (None, false);
                }
                Err(_) => return (None, true), // 其它错误不阻止启动（与 Go 宽容策略一致）
            }
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = name;
        (None, true)
    }
}

// 锁文件记录的进程已不存在时视为崩溃残留。
#[cfg(target_os = "macos")]
fn stale_lock(path: &std::path::Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false; // 读不到内容（空文件/权限）时保守处理：不清理
    };
    let Ok(pid) = text.trim().parse::<i32>() else {
        return false;
    };
    // SAFETY: kill(pid, 0) 只做进程存在性探测，不发送信号。
    unsafe {
        libc::kill(pid, 0);
    }
    let err = std::io::Error::last_os_error();
    // ESRCH = 进程不存在；EPERM = 存在但属其他用户（视为存活）
    err.raw_os_error() == Some(libc::ESRCH)
}

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_fails() {
        // 持缓存目录测试锁：macOS 的锁文件放在缓存目录下，必须保证两次
        // acquire 落在同一个文件上，否则结果随机（已实测踩过）
        let _env = crate::config::test_env_lock();

        // Windows 命名互斥量 / macOS O_EXCL 文件锁：持锁后再获取同一把锁必须失败
        let (g1, ok1) = acquire(&name_of());
        assert!(ok1, "first Acquire should succeed");

        let (_, ok2) = acquire(&name_of());
        assert!(!ok2, "second Acquire should fail while first is held");

        drop(g1);

        let (g3, ok3) = acquire(&name_of());
        assert!(ok3, "Acquire should succeed after release");
        drop(g3);
    }

    fn name_of() -> String {
        format!("keyboard-test-mutex-{}", std::process::id())
    }
}
