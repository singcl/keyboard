// Package singleinstance 通过命名互斥量保证同一时间只运行一个程序实例。
package singleinstance

import (
	"unsafe"

	"golang.org/x/sys/windows"
)

var (
	kernel32         = windows.NewLazySystemDLL("kernel32.dll")
	procCreateMutexW = kernel32.NewProc("CreateMutexW")
)

// Acquire 尝试获取名为 name 的单实例锁。
// ok 为 false 表示已有实例在运行；否则应在程序退出时调用 release。
func Acquire(name string) (release func(), ok bool) {
	namePtr, err := windows.UTF16PtrFromString(name)
	if err != nil {
		return func() {}, true
	}

	h, _, callErr := procCreateMutexW.Call(0, 0, uintptr(unsafe.Pointer(namePtr)))
	handle := windows.Handle(h)
	if handle == 0 {
		// 无法创建互斥量时不阻止启动
		return func() {}, true
	}
	if callErr == windows.ERROR_ALREADY_EXISTS {
		_ = windows.CloseHandle(handle)
		return func() {}, false
	}

	return func() { _ = windows.CloseHandle(handle) }, true
}
