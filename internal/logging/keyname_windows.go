//go:build windows

package logging

import (
	"unsafe"

	"golang.org/x/sys/windows"
)

var (
	user32 = windows.NewLazySystemDLL("user32.dll")

	procGetKeyNameTextW = user32.NewProc("GetKeyNameTextW")
)

// keyName 通过扫描码获取可读的按键名称，失败时返回空字符串。
func keyName(vk, scanCode uint32, extended bool) string {
	lparam := int32(scanCode << 16)
	if extended {
		lparam |= 1 << 24
	}
	buf := make([]uint16, 64)
	n, _, _ := procGetKeyNameTextW.Call(
		uintptr(lparam), uintptr(unsafe.Pointer(&buf[0])), uintptr(len(buf)))
	if n == 0 {
		return ""
	}
	return windows.UTF16ToString(buf[:n])
}
