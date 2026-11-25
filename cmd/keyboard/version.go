//go:build windows

package main

import (
	"fmt"
	"os"
	"unsafe"

	"golang.org/x/sys/windows"
)

var (
	versionDLL = windows.NewLazySystemDLL("version.dll")

	procGetFileVersionInfoSizeW = versionDLL.NewProc("GetFileVersionInfoSizeW")
	procGetFileVersionInfoW     = versionDLL.NewProc("GetFileVersionInfoW")
	procVerQueryValueW          = versionDLL.NewProc("VerQueryValueW")
)

// vsFixedFileInfo 对应 VS_FIXEDFILEINFO，只用到版本字段
type vsFixedFileInfo struct {
	Signature        uint32
	StrucVersion     uint32
	FileVersionMS    uint32
	FileVersionLS    uint32
	ProductVersionMS uint32
	ProductVersionLS uint32
	FileFlagsMask    uint32
	FileFlags        uint32
	FileOS           uint32
	FileType         uint32
	FileSubtype      uint32
	FileDateMS       uint32
	FileDateLS       uint32
}

// appVersion 从 exe 的版本资源中读取主版本号，例如 1.2.0；失败时返回“未知”。
func appVersion() string {
	exe, err := os.Executable()
	if err != nil {
		return "未知"
	}
	path, err := windows.UTF16PtrFromString(exe)
	if err != nil {
		return "未知"
	}

	var handle uint32
	size, _, _ := procGetFileVersionInfoSizeW.Call(uintptr(unsafe.Pointer(path)),
		uintptr(unsafe.Pointer(&handle)))
	if size == 0 {
		return "未知"
	}

	buf := make([]byte, size)
	r, _, _ := procGetFileVersionInfoW.Call(uintptr(unsafe.Pointer(path)), 0,
		uintptr(size), uintptr(unsafe.Pointer(&buf[0])))
	if r == 0 {
		return "未知"
	}

	root, err := windows.UTF16PtrFromString(`\`)
	if err != nil {
		return "未知"
	}
	var info unsafe.Pointer
	var infoLen uint32
	r, _, _ = procVerQueryValueW.Call(uintptr(unsafe.Pointer(&buf[0])),
		uintptr(unsafe.Pointer(root)), uintptr(unsafe.Pointer(&info)),
		uintptr(unsafe.Pointer(&infoLen)))
	if r == 0 || infoLen == 0 || info == nil {
		return "未知"
	}

	fi := (*vsFixedFileInfo)(info)
	return fmt.Sprintf("%d.%d.%d",
		fi.FileVersionMS>>16, fi.FileVersionMS&0xFFFF, fi.FileVersionLS>>16)
}
