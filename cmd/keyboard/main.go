package main

import (
	"bytes"
	"fmt"
	"image/png"
	"os"
	"strings"
	"time"
	"unsafe"

	"github.com/lxn/walk"
	"golang.org/x/sys/windows"

	"keyboard/internal/assets"
	"keyboard/internal/autostart"
	"keyboard/internal/config"
	"keyboard/internal/devices"
	"keyboard/internal/hook"
	"keyboard/internal/logging"
	"keyboard/internal/singleinstance"
)

const (
	mbOK        = 0x00000000
	mbIconError = 0x00000010
)

var (
	user32          = windows.NewLazySystemDLL("user32.dll")
	procMessageBoxW = user32.NewProc("MessageBoxW")
)

func main() {
	release, ok := singleinstance.Acquire("keyboard-misfire-filter")
	if !ok {
		return
	}
	defer release()

	cfg := config.Load()
	debounceMS := cfg.DebounceMS
	paused := false

	mw, err := walk.NewMainWindow()
	if err != nil {
		fatal(err)
	}

	img, err := png.Decode(bytes.NewReader(assets.IconPNG))
	if err != nil {
		fatal(err)
	}
	icon, err := walk.NewIconFromImage(img)
	if err != nil {
		fatal(err)
	}

	ni, err := walk.NewNotifyIcon(mw)
	if err != nil {
		fatal(err)
	}
	defer ni.Dispose()

	if err := ni.SetIcon(icon); err != nil {
		fatal(err)
	}

	var actions = make(map[int]*walk.Action)

	updateTip := func() {
		state := "运行中"
		if paused {
			state = "已暂停"
		}
		_ = ni.SetToolTip(fmt.Sprintf("键盘防抖 %dms · %s", debounceMS, state))
	}

	syncMenu := func() {
		for m, a := range actions {
			_ = a.SetChecked(m == debounceMS)
		}
		updateTip()
	}

	applyDebounce := func(ms int) {
		debounceMS = ms
		hook.SetDebounce(time.Duration(ms) * time.Millisecond)
		syncMenu()
		_ = config.Save(config.Config{DebounceMS: ms})
	}

	// 同一颗键在 ms 毫秒内被重新按下视为误触发并拦截
	for _, ms := range []int{100, 67, 50} {
		ms := ms
		action := walk.NewAction()
		if err := action.SetText(fmt.Sprintf("防抖 %dms", ms)); err != nil {
			fatal(err)
		}
		action.Triggered().Attach(func() { applyDebounce(ms) })
		if err := ni.ContextMenu().Actions().Add(action); err != nil {
			fatal(err)
		}
		actions[ms] = action
	}

	if err := ni.ContextMenu().Actions().Add(walk.NewSeparatorAction()); err != nil {
		fatal(err)
	}

	// 暂停/恢复防抖，勾选表示当前处于暂停状态
	pauseAction := walk.NewAction()
	if err := pauseAction.SetText("暂停防抖"); err != nil {
		fatal(err)
	}
	pauseAction.Triggered().Attach(func() {
		paused = !paused
		hook.SetPaused(paused)
		_ = pauseAction.SetChecked(paused)
		updateTip()
	})
	if err := ni.ContextMenu().Actions().Add(pauseAction); err != nil {
		fatal(err)
	}

	// 开机自启
	autoAction := walk.NewAction()
	if err := autoAction.SetText("开机自启"); err != nil {
		fatal(err)
	}
	_ = autoAction.SetChecked(autostart.Enabled())
	autoAction.Triggered().Attach(func() {
		enable := !autostart.Enabled()
		if err := autostart.Set(enable); err != nil {
			walk.MsgBox(mw, "设置开机自启失败", err.Error(), walk.MsgBoxOK|walk.MsgBoxIconError)
			return
		}
		_ = autoAction.SetChecked(enable)
	})
	if err := ni.ContextMenu().Actions().Add(autoAction); err != nil {
		fatal(err)
	}

	// 识别键盘设备（仅展示，拦截仍对所有键盘生效）
	deviceAction := walk.NewAction()
	if err := deviceAction.SetText("识别键盘设备"); err != nil {
		fatal(err)
	}
	deviceAction.Triggered().Attach(func() {
		list, err := devices.List()
		if err != nil {
			walk.MsgBox(mw, "识别键盘设备失败", err.Error(), walk.MsgBoxOK|walk.MsgBoxIconError)
			return
		}
		if len(list) == 0 {
			walk.MsgBox(mw, "键盘设备", "未检测到键盘设备。", walk.MsgBoxOK|walk.MsgBoxIconInformation)
			return
		}

		var b strings.Builder
		fmt.Fprintf(&b, "检测到 %d 个键盘设备（防抖仍对所有键盘生效）：\n", len(list))
		for i, d := range list {
			fmt.Fprintf(&b, "\n%d. %s", i+1, d.DisplayName())
			if d.VID != "" {
				fmt.Fprintf(&b, "（VID_%s / PID_%s）", d.VID, d.PID)
			}
			fmt.Fprintf(&b, "\n   %s", d.Path)
		}
		walk.MsgBox(mw, "键盘设备", b.String(), walk.MsgBoxOK|walk.MsgBoxIconInformation)
	})
	if err := ni.ContextMenu().Actions().Add(deviceAction); err != nil {
		fatal(err)
	}

	if err := ni.ContextMenu().Actions().Add(walk.NewSeparatorAction()); err != nil {
		fatal(err)
	}

	// 打开拦截日志
	logAction := walk.NewAction()
	if err := logAction.SetText("打开日志"); err != nil {
		fatal(err)
	}
	logAction.Triggered().Attach(func() {
		if err := logging.Open(); err != nil {
			walk.MsgBox(mw, "打开日志失败", err.Error(), walk.MsgBoxOK|walk.MsgBoxIconError)
		}
	})
	if err := ni.ContextMenu().Actions().Add(logAction); err != nil {
		fatal(err)
	}

	// 清空拦截日志
	clearAction := walk.NewAction()
	if err := clearAction.SetText("清空日志"); err != nil {
		fatal(err)
	}
	clearAction.Triggered().Attach(func() {
		if walk.DlgCmdNo == walk.MsgBox(mw, "清空日志", "确定要清空拦截日志吗？",
			walk.MsgBoxYesNo|walk.MsgBoxIconQuestion) {
			return
		}
		if err := logging.Clear(); err != nil {
			walk.MsgBox(mw, "清空日志失败", err.Error(), walk.MsgBoxOK|walk.MsgBoxIconError)
		}
	})
	if err := ni.ContextMenu().Actions().Add(clearAction); err != nil {
		fatal(err)
	}

	if err := ni.ContextMenu().Actions().Add(walk.NewSeparatorAction()); err != nil {
		fatal(err)
	}

	exitAction := walk.NewAction()
	if err := exitAction.SetText("退出"); err != nil {
		fatal(err)
	}
	exitAction.Triggered().Attach(func() { walk.App().Exit(0) })
	if err := ni.ContextMenu().Actions().Add(exitAction); err != nil {
		fatal(err)
	}

	ni.SetVisible(true)

	syncMenu()
	hook.SetDebounce(time.Duration(debounceMS) * time.Millisecond)

	if err := ni.ShowInfo("右键图标设置", "右键图标可设置防抖阈值（默认 100ms）或退出"); err != nil {
		fatal(err)
	}

	go hook.Setup(logging.Blocked)

	// Run the message loop.
	mw.Run()
}

// fatal 在 windowsgui 模式下没有控制台，用弹窗展示启动错误。
func fatal(err error) {
	text, _ := windows.UTF16PtrFromString(err.Error())
	caption, _ := windows.UTF16PtrFromString("keyboard")
	procMessageBoxW.Call(0, uintptr(unsafe.Pointer(text)), uintptr(unsafe.Pointer(caption)), mbOK|mbIconError)
	os.Exit(1)
}
