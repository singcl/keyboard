package main

import (
	"runtime"
	"sync/atomic"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
)

// 低级键盘钩子（纯 Go 实现，不依赖 C）。
// 原理：如果同一颗按键在没有抬起的情况下再次收到按下，说明是按住不放的自动重复，放行；
// 如果抬起后又在很短时间内被重新按下，说明是键盘误触发的重复响应，直接拦截。

const (
	whKeyboardLL = 13 // WH_KEYBOARD_LL

	wmKeyDown    = 0x0100
	wmKeyUp      = 0x0101
	wmSysKeyDown = 0x0104
	wmSysKeyUp   = 0x0105

	mbIconError = 0x00000010
)

var (
	user32 = windows.NewLazySystemDLL("user32.dll")

	procSetWindowsHookEx    = user32.NewProc("SetWindowsHookExW")
	procCallNextHookEx      = user32.NewProc("CallNextHookEx")
	procUnhookWindowsHookEx = user32.NewProc("UnhookWindowsHookEx")
	procGetMessageW         = user32.NewProc("GetMessageW")
	procMessageBoxW         = user32.NewProc("MessageBoxW")

	hookHandle uintptr
	hookProc   uintptr

	// keyDown 记录每个按键当前是否处于按下状态，用于区分“自动重复”与“重新按下”
	keyDown [256]bool

	lastVK   uint32
	lastDown time.Time

	// delayNanos 是同一颗按键两次“重新按下”之间的最小间隔，默认 10 次/秒
	delayNanos int64 = int64(100 * time.Millisecond)
)

// KBDLLHOOKSTRUCT
type kbdLLHookStruct struct {
	VkCode    uint32
	ScanCode  uint32
	Flags     uint32
	Time      uint32
	ExtraInfo uintptr
}

type point struct {
	X, Y int32
}

// MSG，只用于 GetMessage 取出的消息，字段需与 Win32 布局一致
type msg struct {
	Hwnd     uintptr
	Message  uint32
	WParam   uintptr
	LParam   uintptr
	Time     uint32
	Pt       point
	LPrivate uint32
}

// keyboardProc 是 SetWindowsHookExW 的回调，由系统在安装钩子的线程里调用
func keyboardProc(nCode, wParam, lParam uintptr) uintptr {
	if int32(nCode) >= 0 {
		kb := (*kbdLLHookStruct)(unsafe.Pointer(lParam))
		vk := kb.VkCode

		switch uint32(wParam) {
		case wmKeyUp, wmSysKeyUp:
			if vk < 256 {
				keyDown[vk] = false
			}
		case wmKeyDown, wmSysKeyDown:
			// 没有抬起又收到按下，说明是按住不放产生的自动重复
			autoRepeat := vk < 256 && keyDown[vk]
			if vk < 256 {
				keyDown[vk] = true
			}

			// 自动重复直接放行，避免影响长按删除、长按移动光标等操作
			if !autoRepeat {
				now := time.Now()
				delay := time.Duration(atomic.LoadInt64(&delayNanos))
				if !lastDown.IsZero() && vk == lastVK && now.Sub(lastDown) < delay {
					return 1 // 拦截本次误触发
				}
				lastDown = now
				lastVK = vk
			}
		}
	}

	ret, _, _ := procCallNextHookEx.Call(hookHandle, nCode, wParam, lParam)
	return ret
}

// updateKeyPressRate 设置每秒最多允许的击键次数
func updateKeyPressRate(pressRate int) {
	if pressRate <= 0 {
		return
	}
	atomic.StoreInt64(&delayNanos, int64(time.Second)/int64(pressRate))
}

// setupHook 安装钩子并在当前线程跑消息循环。
// 低级键盘钩子要求安装它的线程必须有消息循环，因此需要 LockOSThread。
func setupHook() {
	runtime.LockOSThread()

	hookProc = windows.NewCallback(keyboardProc)
	h, _, err := procSetWindowsHookEx.Call(whKeyboardLL, hookProc, 0, 0)
	if h == 0 {
		text, _ := windows.UTF16PtrFromString("Failed to install hook: " + err.Error())
		caption, _ := windows.UTF16PtrFromString("Error")
		procMessageBoxW.Call(0, uintptr(unsafe.Pointer(text)), uintptr(unsafe.Pointer(caption)), mbIconError)
		return
	}
	hookHandle = h
	defer procUnhookWindowsHookEx.Call(hookHandle)

	var m msg
	for {
		ret, _, _ := procGetMessageW.Call(uintptr(unsafe.Pointer(&m)), 0, 0, 0)
		if int32(ret) <= 0 {
			return
		}
	}
}
