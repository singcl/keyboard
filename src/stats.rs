// 今日拦截计数：钩子线程累加、界面线程读取，跨天自动清零。
//
// Go 版在 Windows 与 macOS 两个入口各写了一份 rollDailyCount/todayCount，
// 这里合并成一个模块。跨天比较用本地日期字符串。

use std::sync::Mutex;

use crate::timeutil;

static STATE: Mutex<Option<(String, i64)>> = Mutex::new(None);

/// 记录一次拦截，返回累加后的今日总数。跨天时先清零。
pub fn add_blocked() -> i64 {
    let mut state = STATE.lock().unwrap();
    roll_locked(&mut state);
    let Some((_, count)) = state.as_mut() else {
        return 0;
    };
    *count += 1;
    *count
}

/// 读取今日拦截总数；跨天且暂无新拦截时返回 0（相当于把昨天的数字清掉）。
pub fn today_count() -> i64 {
    let mut state = STATE.lock().unwrap();
    roll_locked(&mut state);
    state.as_ref().map(|(_, c)| *c).unwrap_or(0)
}

fn roll_locked(state: &mut Option<(String, i64)>) {
    let day = timeutil::now().date();
    match state {
        Some((prev, _)) if prev == &day => {}
        _ => *state = Some((day, 0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 串行执行：模块级单例状态在测试间共享
    #[test]
    fn counts_and_days_roll() {
        let n = add_blocked();
        assert!(n >= 1);
        assert!(today_count() >= 1);
        // 日期字符串格式固定，跨天判断依赖它
        let day = timeutil::now().date();
        assert_eq!(day.len(), 10);
        assert_eq!(day.matches('-').count(), 2);
    }
}
