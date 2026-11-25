// Package logging 负责把被拦截的按键写入日志文件，并支持用系统默认程序打开日志。
//
// 日志位于 %LOCALAPPDATA%\keyboard\keyboard.log，只有真正被拦截的按键才会写入。
// 写入在独立 goroutine 中完成，避免在键盘钩子回调里做磁盘 IO。
package logging

import (
	"fmt"
	"os"
	"path/filepath"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
)

const (
	maxLogSize = 1 << 20 // 1MB，超过后轮转为 keyboard.log.1
	header     = "# keyboard 拦截日志（仅记录被拦截的按键）"
)

// event 是钩子线程投递给日志 goroutine 的一次拦截记录。
type event struct {
	at       time.Time
	vk       uint32
	scanCode uint32
	extended bool
	interval time.Duration
}

var (
	user32 = windows.NewLazySystemDLL("user32.dll")

	procGetKeyNameTextW = user32.NewProc("GetKeyNameTextW")

	events = make(chan event, 256)
)

func init() {
	go writeLoop()
}

// Blocked 记录一次按键拦截。只做入队，不阻塞钩子线程；队列满时直接丢弃。
func Blocked(vk, scanCode uint32, extended bool, interval time.Duration) {
	select {
	case events <- event{time.Now(), vk, scanCode, extended, interval}:
	default:
	}
}

// Open 确保日志文件存在，然后用系统默认程序打开。
func Open() error {
	path, err := Path()
	if err != nil {
		return err
	}

	if info, statErr := os.Stat(path); os.IsNotExist(statErr) || (statErr == nil && info.Size() == 0) {
		if err := appendLine(path, header); err != nil {
			return err
		}
	}

	return windows.ShellExecute(0, nil, windows.StringToUTF16Ptr(path), nil, nil, windows.SW_SHOWNORMAL)
}

// Clear 删除日志文件（含轮转备份）。
func Clear() error {
	path, err := Path()
	if err != nil {
		return err
	}
	if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
		return err
	}
	_ = os.Remove(path + ".1")
	return nil
}

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

func writeLoop() {
	for e := range events {
		write(e)
	}
}

func write(e event) {
	path, err := Path()
	if err != nil {
		return
	}

	if info, err := os.Stat(path); err == nil && info.Size() > maxLogSize {
		_ = os.Remove(path + ".1")
		_ = os.Rename(path, path+".1")
	}

	name := keyName(e.scanCode, e.extended)
	if name == "" {
		name = "-"
	}
	line := fmt.Sprintf("%s\tblocked\t%s\tvk=0x%02X\tscan=0x%02X\tinterval=%.1fms",
		e.at.Format("2006-01-02 15:04:05.000"), name, e.vk, e.scanCode,
		float64(e.interval)/float64(time.Millisecond))

	_ = appendLine(path, line)
}

// appendLine 以追加方式写入一行，写完立即关闭文件，不长期占用日志。
func appendLine(path, line string) error {
	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()

	if info, err := f.Stat(); err == nil && info.Size() == 0 {
		if _, err := fmt.Fprintln(f, header); err != nil {
			return err
		}
	}
	_, err = fmt.Fprintln(f, line)
	return err
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
