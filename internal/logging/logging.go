// Package logging 负责把被拦截的按键写入日志文件，并支持用系统默认程序打开日志。
//
// 日志位于 %LOCALAPPDATA%\keyboard\keyboard.log，只有真正被拦截的按键才会写入。
package logging

import (
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
)

const maxLogSize = 1 << 20 // 1MB，超过后轮转为 keyboard.log.1

var (
	user32 = windows.NewLazySystemDLL("user32.dll")

	procGetKeyNameTextW = user32.NewProc("GetKeyNameTextW")

	logMu   sync.Mutex
	logFile *os.File
)

// Path 返回日志文件路径，并确保所在目录存在。
func Path() (string, error) {
	dir, err := os.UserCacheDir()
	if err != nil {
		return "", err
	}
	dir = filepath.Join(dir, "keyboard")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	return filepath.Join(dir, "keyboard.log"), nil
}

// Blocked 记录一次按键拦截。日志不可用时静默失败，不影响钩子本身。
func Blocked(vk, scanCode uint32, extended bool, interval time.Duration) {
	path, err := Path()
	if err != nil {
		return
	}

	logMu.Lock()
	defer logMu.Unlock()

	if err := openFileLocked(path); err != nil {
		return
	}
	name := keyName(scanCode, extended)
	if name == "" {
		name = "-"
	}
	fmt.Fprintf(logFile, "%s\tblocked\t%s\tvk=0x%02X\tscan=0x%02X\tinterval=%.1fms\n",
		time.Now().Format("2006-01-02 15:04:05.000"), name, vk, scanCode,
		float64(interval)/float64(time.Millisecond))
}

// Open 确保日志文件存在，然后用系统默认程序打开。
func Open() error {
	path, err := Path()
	if err != nil {
		return err
	}

	logMu.Lock()
	err = openFileLocked(path)
	logMu.Unlock()
	if err != nil {
		return err
	}

	return windows.ShellExecute(0, nil, windows.StringToUTF16Ptr(path), nil, nil, windows.SW_SHOWNORMAL)
}

func openFileLocked(path string) error {
	if logFile != nil {
		return nil
	}

	_, statErr := os.Stat(path)
	isNew := os.IsNotExist(statErr)
	if info, err := os.Stat(path); err == nil && info.Size() > maxLogSize {
		_ = os.Remove(path + ".1")
		_ = os.Rename(path, path+".1")
		isNew = true
	}

	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	logFile = f
	if isNew {
		fmt.Fprintln(f, "# keyboard 拦截日志（仅记录被拦截的按键）")
	}
	return nil
}

// keyName 通过扫描码获取可读的按键名称，失败时返回空字符串。
func keyName(scanCode uint32, extended bool) string {
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
