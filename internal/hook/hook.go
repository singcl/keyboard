// Package hook 拦截键盘误触发的重复按键。
//
// 原理：如果同一颗按键在没有抬起的情况下再次收到按下，说明是按住不放的自动重复，放行；
// 如果抬起后又在很短时间内被重新按下，说明是键盘误触发的重复响应，直接拦截。
//
// 拦截能力由各平台实现提供：Windows 见 hook_windows.go（低级键盘钩子），
// 其他平台目前为占位实现。判重逻辑（ShouldBlock）与防抖/暂停状态是平台无关的。
package hook

import (
	"sync/atomic"
	"time"
)

// BlockHandler 在按键被拦截时被调用，参数依次为虚拟键码、扫描码、
// 是否为扩展键、距上一次按下的时间间隔。
type BlockHandler func(vk, scanCode uint32, extended bool, interval time.Duration)

// 最近一次按下的状态，供各平台实现判重
var (
	lastVK   uint32
	lastDown time.Time

	// onBlock 是拦截回调，由 Setup 注入
	onBlock BlockHandler

	// keyDown 记录每个按键当前是否处于按下状态，用于区分“自动重复”与“重新按下”
	keyDown [256]bool

	// delayNanos 是同一颗按键两次“重新按下”之间的最小间隔，默认 10 次/秒
	delayNanos = int64(100 * time.Millisecond)

	// paused 非 0 时暂停拦截，所有按键直接放行
	paused int32
)

// SetDebounce 设置同一颗按键两次“重新按下”之间的最小间隔，<=0 时忽略。
func SetDebounce(d time.Duration) {
	if d <= 0 {
		return
	}
	atomic.StoreInt64(&delayNanos, int64(d))
}

// ShouldBlock 判断一次按下是否属于需要拦截的误触发：
// 同键、非自动重复，且距上次按下小于防抖窗口。
func ShouldBlock(autoRepeat bool, vk, prevVK uint32, since, debounce time.Duration) bool {
	if autoRepeat || prevVK == 0 || vk != prevVK {
		return false
	}
	return since < debounce
}

// SetPaused 暂停或恢复拦截；暂停期间所有按键直接放行。
func SetPaused(pause bool) {
	var v int32
	if pause {
		v = 1
	}
	atomic.StoreInt32(&paused, v)
}

// recordKeyUp 在按键抬起时清理按下状态，各平台实现共用。
func recordKeyUp(vk uint32) {
	if vk < 256 {
		keyDown[vk] = false
	}
}

// shouldBlockNow 判断本次按下是否需要拦截，并更新“最近一次按下”的基准；
// 被拦截时返回距上次按下的间隔。各平台实现共用这套判重逻辑。
func shouldBlockNow(vk uint32) (interval time.Duration, block bool) {
	now := time.Now()
	delay := time.Duration(atomic.LoadInt64(&delayNanos))
	since := now.Sub(lastDown)
	if !lastDown.IsZero() && ShouldBlock(false, vk, lastVK, since, delay) {
		return since, true
	}
	lastDown = now
	lastVK = vk
	return 0, false
}
