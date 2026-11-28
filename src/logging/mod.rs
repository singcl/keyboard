// 拦截日志：把被拦截的按键写入日志文件，并提供路径与清理操作。
//
// 日志位于 %LOCALAPPDATA%\keyboard-plus\keyboard-plus.log（macOS 为
// ~/Library/Caches/keyboard-plus/），只有真正被拦截的按键才会写入。
// 写入在独立线程中完成，避免在键盘钩子回调里做磁盘 IO；队列满时直接丢弃。

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, TrySendError};
use std::time::Duration;

use crate::timeutil;

const MAX_LOG_SIZE: u64 = 1 << 20; // 1MB，超过后轮转为 keyboard-plus.log.1
const HEADER: &str = "# keyboard 日志（记录被拦截的按键与状态事件）";

// 钩子线程投递给日志线程的一次拦截记录或状态事件。
enum Event {
    Blocked {
        at: timeutil::LocalTime,
        vk: u32,
        scan_code: u32,
        extended: bool,
        interval: Duration,
    },
    Note {
        at: timeutil::LocalTime,
        note: String,
    },
}

mod keyname;

// 启动即拉起写入线程；通道满（256）时丢弃新事件，与 Go 版一致。
fn sender() -> &'static mpsc::SyncSender<Event> {
    static SENDER: std::sync::OnceLock<mpsc::SyncSender<Event>> = std::sync::OnceLock::new();
    SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<Event>(256);
        std::thread::Builder::new()
            .name("log-writer".into())
            .spawn(move || {
                while let Ok(e) = rx.recv() {
                    write(e);
                }
            })
            .expect("启动日志线程失败");
        tx
    })
}

/// 记录一次按键拦截。只做入队，不阻塞钩子线程；队列满时直接丢弃。
pub fn blocked(vk: u32, scan_code: u32, extended: bool, interval: Duration) {
    enqueue(Event::Blocked { at: timeutil::now(), vk, scan_code, extended, interval });
}

/// 记录一次状态事件（如目标键盘上下线、自动暂停/恢复）。
pub fn event(note: &str) {
    enqueue(Event::Note { at: timeutil::now(), note: note.to_string() });
}

fn enqueue(e: Event) {
    match sender().try_send(e) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
    }
}

/// 删除日志文件（含轮转备份）。
// 仅 Windows 托盘有「清空日志」菜单项；macOS 版没有该功能（与 Go 版一致）。
#[cfg_attr(not(windows), allow(dead_code))]
pub fn clear() -> io::Result<()> {
    let path = path()?;
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let _ = fs::remove_file(backup_path(&path));
    Ok(())
}

/// 返回日志文件路径，并确保所在目录存在。
pub fn path() -> io::Result<PathBuf> {
    path_in(&crate::config::base_cache_dir()?)
}

pub(crate) fn path_in(base: &std::path::Path) -> io::Result<PathBuf> {
    let dir = base.join("keyboard-plus");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("keyboard-plus.log"))
}

fn backup_path(path: &std::path::Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".1");
    PathBuf::from(s)
}

fn write(e: Event) {
    let Ok(path) = path() else { return };

    if let Ok(info) = fs::metadata(&path) {
        if info.len() > MAX_LOG_SIZE {
            let _ = fs::remove_file(backup_path(&path));
            let _ = fs::rename(&path, backup_path(&path));
        }
    }

    let line = match &e {
        Event::Note { at, note } => {
            format!("{}\tevent\t{}", at.datetime(), note)
        }
        Event::Blocked { at, vk, scan_code, extended, interval } => {
            let mut name = keyname::key_name(*vk, *scan_code, *extended);
            if name.is_empty() {
                name = "-".to_string();
            }
            format!(
                "{}\tblocked\t{}\tvk=0x{:02X}\tscan=0x{:02X}\tinterval={:.1}ms",
                at.datetime(),
                name,
                vk,
                scan_code,
                interval.as_secs_f64() * 1000.0
            )
        }
    };

    let _ = append_line(&path, &line);
}

// 以追加方式写入一行，写完立即关闭文件，不长期占用日志。
fn append_line(path: &std::path::Path, line: &str) -> io::Result<()> {
    let mut f = OpenOptions::new().append(true).create(true).open(path)?;

    if f.metadata()?.len() == 0 {
        writeln!(f, "{HEADER}")?;
    }
    writeln!(f, "{line}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn async_write_reaches_file() {
        // 与其它读缓存目录的测试互斥（覆盖是全局的，并行会互相污染）
        let _env = crate::config::test_env_lock();
        let dir = std::env::temp_dir().join(format!("kb-plus-logtest-{}", std::process::id()));
        // 把写入线程的「用户缓存目录」指到临时目录（等价于 Go 版 t.Setenv）
        crate::config::set_base_dir_for_tests(&dir);
        let path = path_in(&dir).unwrap();
        let _ = fs::remove_file(&path);

        blocked(0x41, 0x1E, false, Duration::from_micros(3200));

        // 写入线程异步落盘，轮询等待内容出现
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Ok(text) = fs::read_to_string(&path) {
                if text.contains("vk=0x41") {
                    assert!(text.contains("interval=3.2ms"), "内容：{text}");
                    assert!(text.contains(HEADER), "首次写入应带文件头：{text}");
                    return;
                }
            }
            if std::time::Instant::now() >= deadline {
                panic!("log entry not written");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}
