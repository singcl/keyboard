// 本地时间格式化：只实现日志与跨天判断需要的「YYYY-MM-DD」和「YYYY-MM-DD HH:MM:SS.mmm」。
//
// 不引入 chrono 之类的依赖：Go 版用的是本地时间，std 只有 UTC，
// 因此按平台取本地时间——Windows 用 GetLocalTime，macOS 用 libc 的 localtime_r。

/// 一次本地时间采样，字段含义与 SYSTEMTIME / tm 一致。
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millis: u32,
}

impl LocalTime {
    /// 「2026-01-02」，用于跨天判断。
    pub fn date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// 「2026-01-02 15:04:05.000」，与 Go 版日志时间戳格式一致。
    pub fn datetime(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
            self.year, self.month, self.day, self.hour, self.minute, self.second, self.millis
        )
    }
}

/// 取当前本地时间。
pub fn now() -> LocalTime {
    #[cfg(windows)]
    {
        // SAFETY: GetLocalTime 无特殊约束。
        let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        LocalTime {
            year: st.wYear as i32,
            month: st.wMonth as u32,
            day: st.wDay as u32,
            hour: st.wHour as u32,
            minute: st.wMinute as u32,
            second: st.wSecond as u32,
            millis: st.wMilliseconds as u32,
        }
    }

    #[cfg(target_os = "macos")]
    {
        let secs = now_secs();
        // SAFETY: tm 初始化为零后传入 localtime_r，二者均不涉及指针别名问题。
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe {
            libc::localtime_r(&secs, &mut tm);
        }
        LocalTime {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u32,
            day: tm.tm_mday as u32,
            hour: tm.tm_hour as u32,
            minute: tm.tm_min as u32,
            second: tm.tm_sec as u32,
            millis: (now_nanos() % 1_000_000) as u32 / 1_000,
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let nanos = unix_nanos();
        unix_to_local(nanos, 0)
    }
}

#[cfg(target_os = "macos")]
fn now_secs() -> libc::time_t {
    // SAFETY: time(NULL) 无特殊约束。
    unsafe { libc::time(std::ptr::null_mut()) }
}

#[cfg(target_os = "macos")]
fn now_nanos() -> i64 {
    unix_nanos()
}

// UNIX 纪元以来的纳秒数；测试与占位实现共用。
pub(crate) fn unix_nanos() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

// 把 UNIX 纳秒加上本地时区偏移（秒）后转成 UTC 分解，作为无 libc 平台的占位实现。
#[cfg(not(any(windows, target_os = "macos")))]
fn unix_to_local(nanos: i64, offset_secs: i64) -> LocalTime {
    let days = (nanos / 1_000_000_000 + offset_secs as i64) / 86400;
    let secs_of_day = (nanos / 1_000_000_000 + offset_secs).rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    LocalTime {
        year: y,
        month: m as u32,
        day: d as u32,
        hour: (secs_of_day / 3600) as u32,
        minute: (secs_of_day / 60 % 60) as u32,
        second: (secs_of_day % 60) as u32,
        millis: (nanos % 1_000_000_000 / 1_000_000) as u32,
    }
}

// Howard Hinnant 的 civil_from_days：UNIX 天数转公历日期。
#[cfg(not(any(windows, target_os = "macos")))]
fn civil_from_days(z: i64) -> (i32, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    ((y + i64::from(m <= 2)) as i32, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_and_datetime_format() {
        let t = LocalTime { year: 2026, month: 1, day: 2, hour: 15, minute: 4, second: 5, millis: 7 };
        assert_eq!(t.date(), "2026-01-02");
        assert_eq!(t.datetime(), "2026-01-02 15:04:05.007");
    }

    #[test]
    fn now_is_sane() {
        let t = now();
        assert!((2020..2100).contains(&t.year), "{}", t.year);
        assert!((1..=12).contains(&t.month));
        assert!((1..=31).contains(&t.day));
    }
}
