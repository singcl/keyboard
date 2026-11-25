//go:build windows

package hook

import (
	"fmt"
	"runtime"
	"sync/atomic"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
)

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
)

// kbdLLHookStruct 对应 Win32 的 KBDLLHOOKSTRUCT
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

// msg 对应 Win32 的 MSG，只用于 GetMessage 取出的消息，字段需与 Win32 布局一致
type msg struct {
	Hwnd     uintptr
	Message  uint32
	WParam   uintptr
	LParam   uintptr
	Time     uint32
	Pt       point
	LPrivate uint32
}

// Setup 安装钩子并在当前 goroutine 上跑消息循环，正常情况下不会返回。
// handler 可为 nil；低级键盘钩子要求安装它的线程必须有消息循环，因此内部会 LockOSThread。
func Setup(handler BlockHandler) error {
	if handler == nil {
		handler = func(uint32, uint32, bool, time.Duration) {}
	}
	onBlock = handler

	runtime.LockOSThread()

	hookProc = windows.NewCallback(keyboardProc)
	h, _, err := procSetWindowsHookEx.Call(whKeyboardLL, hookProc, 0, 0)
	if h == 0 {
		text, _ := windows.UTF16PtrFromString("Failed to install hook: " + err.Error())
		caption, _ := windows.UTF16PtrFromString("Error")
		procMessageBoxW.Call(0, uintptr(unsafe.Pointer(text)), uintptr(unsafe.Pointer(caption)), mbIconError)
		return fmt.Errorf("hook: 安装键盘钩子失败：%w", err)
	}
	hookHandle = h
	defer procUnhookWindowsHookEx.Call(hookHandle)

	var m msg
	for {
		ret, _, _ := procGetMessageW.Call(uintptr(unsafe.Pointer(&m)), 0, 0, 0)
		if int32(ret) <= 0 {
			return nil
		}
	}
}

// keyboardProc 是 SetWindowsHookExW 的回调，由系统在安装钩子的线程里调用
func keyboardProc(nCode, wParam, lParam uintptr) uintptr {
	if int32(nCode) >= 0 && atomic.LoadInt32(&paused) == 0 {
		// lParam 是系统传入的 KBDLLHOOKSTRUCT 地址；先取出其中保存的指针再转换类型，
		// 避免 go vet 对 uintptr 直接转 unsafe.Pointer 的误报
		kb := (*kbdLLHookStruct)(*(*unsafe.Pointer)(unsafe.Pointer(&lParam)))
		vk := kb.VkCode

		switch uint32(wParam) {
		case wmKeyUp, wmSysKeyUp:
			recordKeyUp(vk)
		case wmKeyDown, wmSysKeyDown:
			// 没有抬起又收到按下，说明是按住不放产生的自动重复
			autoRepeat := vk < 256 && keyDown[vk]
			if vk < 256 {
				keyDown[vk] = true
			}

			// 自动重复直接放行，避免影响长按删除、长按移动光标等操作
			if !autoRepeat {
				if since, block := shouldBlockNow(vk); block {
					onBlock(vk, kb.ScanCode, kb.Flags&1 != 0, since)
					return 1 // 拦截本次误触发
				}
			}
		}
	}

	ret, _, _ := procCallNextHookEx.Call(hookHandle, nCode, wParam, lParam)
	return ret
}
