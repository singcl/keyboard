//go:build darwin

package hook

/*
#cgo LDFLAGS: -framework CoreGraphics -framework CoreFoundation -framework ApplicationServices
#include <stdint.h>
#include <stdbool.h>

// 以下函数由 hook_darwin.c 实现。
int kbdTapStart(void);
void kbdTapStop(void);
int kbdAccessTrusted(void);
int kbdAccessPrompt(void);
*/
import "C"

import (
	"errors"
	"sync/atomic"
	"time"
)

// Setup 安装全局键盘事件监听（CGEventTap）。
//
// 与 Windows 不同，这里安装完成后立即返回：事件由主线程的 run loop 驱动，
// 因此调用方需要在主线程启动 AppKit 的事件循环（NSApplication run）。
// 返回错误表示安装失败，通常是缺少「输入监控」权限。
func Setup(handler BlockHandler) error {
	if handler == nil {
		handler = func(uint32, uint32, bool, time.Duration) {}
	}
	onBlock = handler

	// 没有权限时先触发系统的授权引导，用户只需在系统设置里勾选本程序
	if C.kbdAccessTrusted() == 0 {
		C.kbdAccessPrompt()
	}

	switch C.kbdTapStart() {
	case 0:
		return nil
	case 1:
		return errors.New("hook: 没有键盘监听权限，请在「系统设置 → 隐私与安全性 → 输入监控」中勾选本程序后重新启动")
	default:
		return errors.New("hook: 无法创建键盘事件监听")
	}
}

// Stop 移除全局键盘事件监听，退出程序前调用。
func Stop() {
	C.kbdTapStop()
}

// goHandleKeyCode 由 CGEventTap 的回调在主线程调用，返回 1 表示拦截这次按键。
// keyCode 是 macOS 的虚拟键码，autoRepeat 表示按住不放产生的自动重复。
//
//export goHandleKeyCode
func goHandleKeyCode(keyCode uint32, down, autoRepeat int) int {
	if atomic.LoadInt32(&paused) != 0 || down == 0 || autoRepeat != 0 {
		return 0
	}
	if since, block := shouldBlockNow(keyCode); block {
		onBlock(keyCode, 0, false, since)
		return 1
	}
	return 0
}
