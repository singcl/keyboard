//go:build darwin

// Command keyboardmac 是键盘防抖的 macOS 版本：菜单栏图标 + CGEventTap 全局拦截。
//
// 与 Windows 版本不同，macOS 的低级事件监听（CGEventTap）需要「输入监控」权限，
// 且事件回调由主线程的 run loop 驱动，因此入口在主线程上依次初始化 AppKit 和事件监听。
package main

/*
#cgo LDFLAGS: -framework Cocoa

#include <stdlib.h>

// 以下函数由 cocoa_darwin.m 实现。
void uiInit(const void *normalPNG, int normalLen, const void *grayPNG, int grayLen, const char *version);
void uiRun(void);
void uiRefresh(void);
void uiAlert(const char *title, const char *message);
void uiPermissionAlert(void);
*/
import "C"

import (
	"bytes"
	"fmt"
	"image"
	"image/draw"
	"image/png"
	"os"
	"os/exec"
	"runtime"
	"sync"
	"sync/atomic"
	"time"
	"unsafe"

	"keyboard/internal/assets"
	"keyboard/internal/autostart"
	"keyboard/internal/config"
	"keyboard/internal/hook"
	"keyboard/internal/logging"
	"keyboard/internal/singleinstance"
)

// appVersion 是程序版本，发布构建时通过 -ldflags "-X main.appVersion=..." 覆盖。
var appVersion = "dev"

var (
	// cfg 在主线程与界面回调之间共享，用互斥锁保护
	cfgMu sync.Mutex
	cfg   config.Config

	// userPaused 是用户手动暂停的状态，与配置里的 Paused 同步
	userPaused int32

	// tapActive 表示事件监听是否安装成功
	tapActive int32

	// 今日拦截计数；事件回调与界面都在主线程，计数仍用原子操作以便安全读取
	blockedToday int64
	blockedDate  string
)

func main() {
	// 菜单栏与事件监听都必须运行在主线程的 run loop 上
	runtime.LockOSThread()

	release, ok := singleinstance.Acquire("keyboard-misfire-filter")
	if !ok {
		showAlert("键盘防抖", "程序已在运行，请在菜单栏的键盘图标中操作。")
		return
	}
	defer release()

	cfg = config.Load()
	atomic.StoreInt32(&userPaused, boolToInt32(cfg.Paused))

	normal := assets.IconPNG
	gray := grayPNG(normal)
	versionC := C.CString("版本 " + appVersion)
	C.uiInit(
		unsafe.Pointer(&normal[0]), C.int(len(normal)),
		unsafe.Pointer(&gray[0]), C.int(len(gray)),
		versionC,
	)
	C.free(unsafe.Pointer(versionC))

	// 恢复上次的暂停状态
	hook.SetPaused(cfg.Paused)

	if err := hook.Setup(countBlock); err != nil {
		logging.Event(err.Error())
		C.uiPermissionAlert()
	} else {
		atomic.StoreInt32(&tapActive, 1)
	}
	C.uiRefresh()

	// 事件由主线程 run loop 驱动，正常情况下不会返回
	C.uiRun()
}

// countBlock 统计并记录被拦截的按键，同时刷新菜单栏状态。
// 它在事件监听回调里执行，始终位于主线程。
func countBlock(vk, scanCode uint32, extended bool, interval time.Duration) {
	day := time.Now().Format("2006-01-02")
	if blockedDate != day {
		blockedDate = day
		atomic.StoreInt64(&blockedToday, 0)
	}
	atomic.AddInt64(&blockedToday, 1)

	logging.Blocked(vk, scanCode, extended, interval)
	C.uiRefresh()
}

// goState 返回菜单栏状态：0 运行中、1 已暂停、2 未生效（缺少权限）。
//
//export goState
func goState() C.int {
	if atomic.LoadInt32(&tapActive) == 0 {
		return 2
	}
	return goPaused()
}

// goPaused 返回用户是否手动暂停了拦截。
//
//export goPaused
func goPaused() C.int {
	return C.int(atomic.LoadInt32(&userPaused))
}

//export goBlockedToday
func goBlockedToday() C.int {
	return C.int(atomic.LoadInt64(&blockedToday))
}

//export goDebounceMS
func goDebounceMS() C.int {
	cfgMu.Lock()
	defer cfgMu.Unlock()
	return C.int(cfg.DebounceMS)
}

//export goTogglePause
func goTogglePause() {
	cfgMu.Lock()
	cfg.Paused = !cfg.Paused
	paused := cfg.Paused
	_ = config.Save(cfg)
	cfgMu.Unlock()

	atomic.StoreInt32(&userPaused, boolToInt32(paused))
	hook.SetPaused(paused)
	if paused {
		logging.Event("已手动暂停拦截")
	} else {
		logging.Event("已手动恢复拦截")
	}
}

//export goSetDebounce
func goSetDebounce(ms C.int) {
	value := int(ms)
	if value < 10 || value > 1000 {
		showAlert("数值无效", "请输入 10-1000 之间的整数。")
		return
	}

	cfgMu.Lock()
	cfg.DebounceMS = value
	_ = config.Save(cfg)
	cfgMu.Unlock()

	hook.SetDebounce(time.Duration(value) * time.Millisecond)
	logging.Event(fmt.Sprintf("防抖阈值调整为 %dms", value))
}

//export goAutostartEnabled
func goAutostartEnabled() C.int {
	if autostart.Enabled() {
		return 1
	}
	return 0
}

//export goToggleAutostart
func goToggleAutostart() {
	enable := !autostart.Enabled()
	if err := autostart.Set(enable); err != nil {
		showAlert("设置开机自启失败", err.Error())
		return
	}
	if enable {
		logging.Event("已开启开机自启")
	} else {
		logging.Event("已关闭开机自启")
	}
}

//export goOpenLog
func goOpenLog() {
	path, err := logging.Path()
	if err != nil {
		showAlert("打开日志失败", err.Error())
		return
	}
	if _, err := os.Stat(path); err != nil {
		showAlert("暂无日志", "目前还没有拦截记录。")
		return
	}
	if err := exec.Command("open", path).Start(); err != nil {
		showAlert("打开日志失败", err.Error())
	}
}

//export goRevealConfig
func goRevealConfig() {
	path, err := config.Path()
	if err != nil {
		showAlert("定位配置文件失败", err.Error())
		return
	}
	if err := exec.Command("open", "-R", path).Start(); err != nil {
		showAlert("定位配置文件失败", err.Error())
	}
}

//export goQuit
func goQuit() {
	hook.Stop()
	os.Exit(0)
}

// grayPNG 把彩色图标转成灰度 PNG，作为暂停状态的图标。
func grayPNG(src []byte) []byte {
	img, err := png.Decode(bytes.NewReader(src))
	if err != nil {
		return src
	}
	gray := image.NewGray(img.Bounds())
	draw.Draw(gray, gray.Bounds(), img, img.Bounds().Min, draw.Src)

	var buf bytes.Buffer
	if err := png.Encode(&buf, gray); err != nil {
		return src
	}
	return buf.Bytes()
}

func boolToInt32(v bool) int32 {
	if v {
		return 1
	}
	return 0
}

// showAlert 弹出系统提示框，可在界面初始化前后调用。
func showAlert(title, message string) {
	titleC := C.CString(title)
	messageC := C.CString(message)
	defer C.free(unsafe.Pointer(titleC))
	defer C.free(unsafe.Pointer(messageC))
	C.uiAlert(titleC, messageC)
}
