// Package logging 负责把被拦截的按键写入日志文件，并提供路径与清理操作。
//
// 日志位于 %LOCALAPPDATA%\keyboard\keyboard.log，只有真正被拦截的按键才会写入。
// 写入在独立 goroutine 中完成，避免在键盘钩子回调里做磁盘 IO。
package logging

import (
	"fmt"
	"os"
	"path/filepath"
	"time"
)

const (
	maxLogSize = 1 << 20 // 1MB，超过后轮转为 keyboard.log.1
	header     = "# keyboard 日志（记录被拦截的按键与状态事件）"
)

// event 是钩子线程投递给日志 goroutine 的一次拦截记录或状态事件。
type event struct {
	at       time.Time
	vk       uint32
	scanCode uint32
	extended bool
	interval time.Duration
	note     string
}

var events = make(chan event, 256)

func init() {
	go writeLoop()
}

// Blocked 记录一次按键拦截。只做入队，不阻塞钩子线程；队列满时直接丢弃。
func Blocked(vk, scanCode uint32, extended bool, interval time.Duration) {
	select {
	case events <- event{at: time.Now(), vk: vk, scanCode: scanCode, extended: extended, interval: interval}:
	default:
	}
}

// Event 记录一次状态事件（如目标键盘上下线、自动暂停/恢复）。
func Event(note string) {
	select {
	case events <- event{at: time.Now(), note: note}:
	default:
	}
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

	if e.note != "" {
		_ = appendLine(path, fmt.Sprintf("%s\tevent\t%s",
			e.at.Format("2006-01-02 15:04:05.000"), e.note))
		return
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
