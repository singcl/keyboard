//go:build windows

package main

import (
	"bytes"
	"fmt"
	"image"
	"image/draw"
	"image/png"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
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
	mbOK              = 0x00000000
	mbIconError       = 0x00000010
	mbIconInformation = 0x00000040
)

var (
	user32          = windows.NewLazySystemDLL("user32.dll")
	procMessageBoxW = user32.NewProc("MessageBoxW")
)

func main() {
	release, ok := singleinstance.Acquire("keyboard-plus")
	if !ok {
		// 已有实例在运行，提示用户从托盘操作，避免双击后“毫无反应”
		text, _ := windows.UTF16PtrFromString("程序已在运行，请在任务栏右下角的托盘图标中操作。")
		caption, _ := windows.UTF16PtrFromString("键盘防抖")
		procMessageBoxW.Call(0, uintptr(unsafe.Pointer(text)),
			uintptr(unsafe.Pointer(caption)), mbOK|mbIconInformation)
		return
	}
	defer release()

	cfg := config.Load()

	// 配置在主线程写入、看门狗 goroutine 读取，用互斥锁保护
	var cfgMu sync.Mutex
	cfgSnapshot := func() (bool, string, string) {
		cfgMu.Lock()
		defer cfgMu.Unlock()
		return cfg.OnlyWhenConnected, cfg.TargetName, cfg.TargetPath
	}
	updateCfg := func(update func(*config.Config)) {
		cfgMu.Lock()
		update(&cfg)
		_ = config.Save(cfg)
		cfgMu.Unlock()
	}

	userPaused := cfg.Paused
	targetOnline := true

	// 今日拦截计数：钩子线程累加，UI 线程读取
	var blockedToday int64
	var blockedDate atomic.Value

	targetConfigured := func() bool {
		return cfg.TargetName != "" || cfg.TargetPath != ""
	}
	if cfg.OnlyWhenConnected && targetConfigured() {
		targetOnline = devices.Online(cfg.TargetName, cfg.TargetPath)
		if !targetOnline {
			logging.Event("启动时目标键盘未连接，暂不拦截")
		}
	}

	effectivePaused := func() bool {
		return userPaused || (cfg.OnlyWhenConnected && targetConfigured() && !targetOnline)
	}

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

	// 暂停时使用灰度图标，便于一眼区分状态
	gray := image.NewGray(img.Bounds())
	draw.Draw(gray, gray.Bounds(), img, img.Bounds().Min, draw.Src)
	pausedIcon, err := walk.NewIconFromImage(gray)
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

	// 菜单顶部状态行，直接显示当前是否在拦截
	statusAction := walk.NewAction()
	if err := statusAction.SetEnabled(false); err != nil {
		fatal(err)
	}
	if err := ni.ContextMenu().Actions().Add(statusAction); err != nil {
		fatal(err)
	}
	if err := ni.ContextMenu().Actions().Add(walk.NewSeparatorAction()); err != nil {
		fatal(err)
	}

	var actions = make(map[int]*walk.Action)

	updateTip := func() {
		state := "运行中"
		switch {
		case userPaused:
			state = "已暂停"
		case cfg.OnlyWhenConnected && targetConfigured() && !targetOnline:
			state = "已暂停（目标键盘未连接）"
		}
		text := "状态：" + state
		if n := atomic.LoadInt64(&blockedToday); n > 0 {
			text += fmt.Sprintf(" · 今日拦截 %d 次", n)
		}
		_ = statusAction.SetText(text)
		_ = ni.SetToolTip(fmt.Sprintf("键盘防抖 %dms · %s", cfg.DebounceMS, state))
	}

	var customAction *walk.Action
	var debounceMenuAction *walk.Action

	syncMenu := func() {
		custom := true
		for m, a := range actions {
			_ = a.SetChecked(m == cfg.DebounceMS)
			if m == cfg.DebounceMS {
				custom = false
			}
		}
		if customAction != nil {
			_ = customAction.SetChecked(custom)
			// 自定义阈值时直接显示当前值，不用打开输入框才能看到
			if custom {
				_ = customAction.SetText(fmt.Sprintf("自定义阈值（%dms）…", cfg.DebounceMS))
			} else {
				_ = customAction.SetText("自定义阈值…")
			}
		}
		// 子菜单标题带上当前阈值，收起时也能看到
		if debounceMenuAction != nil {
			_ = debounceMenuAction.SetText(fmt.Sprintf("防抖阈值（%dms）", cfg.DebounceMS))
		}
		updateTip()
	}

	applyDebounce := func(ms int) {
		updateCfg(func(c *config.Config) { c.DebounceMS = ms })
		hook.SetDebounce(time.Duration(ms) * time.Millisecond)
		syncMenu()
		logging.Event(fmt.Sprintf("防抖阈值调整为 %dms", ms))
	}

	// 防抖阈值放进子菜单，和「目标键盘」一致，避免顶层菜单过长
	debounceMenu, err := walk.NewMenu()
	if err != nil {
		fatal(err)
	}
	debounceMenuAction, err = ni.ContextMenu().Actions().AddMenu(debounceMenu)
	if err != nil {
		fatal(err)
	}
	if err := debounceMenuAction.SetText("防抖阈值"); err != nil {
		fatal(err)
	}

	// 同一颗键在 ms 毫秒内被重新按下视为误触发并拦截
	for _, ms := range []int{100, 80, 67, 50} {
		ms := ms
		action := walk.NewAction()
		if err := action.SetText(fmt.Sprintf("%dms", ms)); err != nil {
			fatal(err)
		}
		action.Triggered().Attach(func() { applyDebounce(ms) })
		if err := debounceMenu.Actions().Add(action); err != nil {
			fatal(err)
		}
		actions[ms] = action
	}

	// 自定义阈值：弹出输入框，允许 10-1000 毫秒
	customAction = walk.NewAction()
	if err := customAction.SetText("自定义阈值…"); err != nil {
		fatal(err)
	}
	customAction.Triggered().Attach(func() {
		if ms, ok := askDebounce(cfg.DebounceMS); ok {
			applyDebounce(ms)
		}
	})
	if err := debounceMenu.Actions().Add(customAction); err != nil {
		fatal(err)
	}

	if err := ni.ContextMenu().Actions().Add(walk.NewSeparatorAction()); err != nil {
		fatal(err)
	}

	// 暂停/恢复防抖，勾选表示手动暂停
	pauseAction := walk.NewAction()
	if err := pauseAction.SetText("暂停防抖"); err != nil {
		fatal(err)
	}

	applyPauseState := func() {
		paused := effectivePaused()
		hook.SetPaused(paused)
		_ = pauseAction.SetChecked(userPaused)
		if paused {
			_ = ni.SetIcon(pausedIcon)
		} else {
			_ = ni.SetIcon(icon)
		}
		updateTip()
	}

	pauseAction.Triggered().Attach(func() {
		userPaused = !userPaused
		updateCfg(func(c *config.Config) { c.Paused = userPaused })
		applyPauseState()
		if userPaused {
			logging.Event("已手动暂停拦截")
		} else {
			logging.Event("已手动恢复拦截")
		}
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
		if enable {
			logging.Event("已开启开机自启")
		} else {
			logging.Event("已关闭开机自启")
		}
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

		walk.MsgBox(mw, "键盘设备",
			devices.FormatList(list, cfg.TargetName, cfg.TargetPath),
			walk.MsgBoxOK|walk.MsgBoxIconInformation)
	})
	if err := ni.ContextMenu().Actions().Add(deviceAction); err != nil {
		fatal(err)
	}

	// 仅当目标键盘在线时才启用防抖（拦截本身仍是全局的）
	gateAction := walk.NewAction()
	if err := gateAction.SetText("仅目标键盘在线时防抖"); err != nil {
		fatal(err)
	}
	_ = gateAction.SetChecked(cfg.OnlyWhenConnected)
	gateAction.Triggered().Attach(func() {
		enable := !cfg.OnlyWhenConnected
		if enable && !targetConfigured() {
			_ = gateAction.SetChecked(false)
			walk.MsgBox(mw, "未设置目标键盘",
				"请先在「目标键盘」子菜单中选择要匹配的设备。", walk.MsgBoxOK|walk.MsgBoxIconInformation)
			return
		}
		updateCfg(func(c *config.Config) { c.OnlyWhenConnected = enable })
		targetOnline = true
		if enable {
			targetOnline = devices.Online(cfg.TargetName, cfg.TargetPath)
		}
		_ = gateAction.SetChecked(enable)
		if enable {
			logging.Event("已开启仅目标键盘在线时防抖")
		} else {
			logging.Event("已关闭仅目标键盘在线时防抖")
		}
		applyPauseState()
	})
	if err := ni.ContextMenu().Actions().Add(gateAction); err != nil {
		fatal(err)
	}

	// 目标键盘子菜单：列出当前设备，点击即设为匹配目标
	targetMenu, err := walk.NewMenu()
	if err != nil {
		fatal(err)
	}
	targetMenuAction, err := ni.ContextMenu().Actions().AddMenu(targetMenu)
	if err != nil {
		fatal(err)
	}
	if err := targetMenuAction.SetText("目标键盘"); err != nil {
		fatal(err)
	}

	var rebuildTargetMenu func()
	rebuildTargetMenu = func() {
		_ = targetMenu.Actions().Clear()

		list, err := devices.List()
		if err != nil || len(list) == 0 {
			empty := walk.NewAction()
			_ = empty.SetText("未检测到键盘设备")
			_ = empty.SetEnabled(false)
			_ = targetMenu.Actions().Add(empty)
			return
		}

		addItem := func(d devices.Device, label string) {
			action := walk.NewAction()
			_ = action.SetText(label)
			_ = action.SetChecked(d.Matches(cfg.TargetName, cfg.TargetPath))
			action.Triggered().Attach(func() {
				updateCfg(func(c *config.Config) {
					c.TargetName = d.Name
					c.TargetPath = d.Path
				})
				targetOnline = true
				if cfg.OnlyWhenConnected {
					targetOnline = devices.Online(d.Name, d.Path)
				}
				applyPauseState()
				logging.Event("目标键盘切换为：" + label)
				rebuildTargetMenu()
			})
			_ = targetMenu.Actions().Add(action)
		}

		physical, system := devices.Group(list)
		physicalLabels := devices.Labels(physical)
		for i, d := range physical {
			addItem(d, physicalLabels[i])
		}
		if len(physical) == 0 {
			none := walk.NewAction()
			_ = none.SetText("未检测到实体键盘")
			_ = none.SetEnabled(false)
			_ = targetMenu.Actions().Add(none)
		}
		if len(system) > 0 {
			_ = targetMenu.Actions().Add(walk.NewSeparatorAction())
			header := walk.NewAction()
			_ = header.SetText("系统 / 虚拟设备")
			_ = header.SetEnabled(false)
			_ = targetMenu.Actions().Add(header)
			systemLabels := devices.Labels(system)
			for i, d := range system {
				addItem(d, systemLabels[i])
			}
		}
	}
	rebuildTargetMenu()

	// 每次右键打开菜单前刷新设备列表与开机自启状态，保证菜单与实际一致
	ni.MouseUp().Attach(func(x, y int, button walk.MouseButton) {
		if button == walk.RightButton {
			_ = autoAction.SetChecked(autostart.Enabled())
			rebuildTargetMenu()
			syncMenu()
		}
	})

	if err := ni.ContextMenu().Actions().Add(walk.NewSeparatorAction()); err != nil {
		fatal(err)
	}

	// 查看拦截日志（内置窗口）
	logAction := walk.NewAction()
	if err := logAction.SetText("查看日志"); err != nil {
		fatal(err)
	}
	logAction.Triggered().Attach(func() { showLogView(mw) })
	if err := ni.ContextMenu().Actions().Add(logAction); err != nil {
		fatal(err)
	}

	// 在资源管理器中定位配置文件
	cfgLocAction := walk.NewAction()
	if err := cfgLocAction.SetText("配置文件位置"); err != nil {
		fatal(err)
	}
	cfgLocAction.Triggered().Attach(func() {
		path, err := config.Path()
		if err != nil {
			walk.MsgBox(mw, "定位配置文件失败", err.Error(), walk.MsgBoxOK|walk.MsgBoxIconError)
			return
		}
		target := path
		if _, statErr := os.Stat(path); statErr != nil {
			target = filepath.Dir(path)
		}
		if err := windows.ShellExecute(0, nil, windows.StringToUTF16Ptr(target), nil, nil, windows.SW_SHOWNORMAL); err != nil {
			walk.MsgBox(mw, "定位配置文件失败", err.Error(), walk.MsgBoxOK|walk.MsgBoxIconError)
		}
	})
	if err := ni.ContextMenu().Actions().Add(cfgLocAction); err != nil {
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

	versionAction := walk.NewAction()
	if err := versionAction.SetText("版本 v" + appVersion()); err != nil {
		fatal(err)
	}
	if err := versionAction.SetEnabled(false); err != nil {
		fatal(err)
	}
	if err := ni.ContextMenu().Actions().Add(versionAction); err != nil {
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
	hook.SetDebounce(time.Duration(cfg.DebounceMS) * time.Millisecond)
	applyPauseState()

	if err := ni.ShowInfo("右键图标设置", "右键图标可设置防抖阈值、目标键盘或退出"); err != nil {
		fatal(err)
	}

	// 统计今日拦截次数，同时写入拦截日志
	countBlock := func(vk, scanCode uint32, extended bool, interval time.Duration) {
		day := time.Now().Format("2006-01-02")
		if prev, _ := blockedDate.Load().(string); prev != day {
			blockedDate.Store(day)
			atomic.StoreInt64(&blockedToday, 0)
		}
		atomic.AddInt64(&blockedToday, 1)
		logging.Blocked(vk, scanCode, extended, interval)
	}
	go hook.Setup(countBlock)

	// 定时检测目标键盘是否在线，自动暂停或恢复拦截
	go func() {
		ticker := time.NewTicker(2 * time.Second)
		defer ticker.Stop()
		for range ticker.C {
			mw.Synchronize(updateTip)

			enabled, name, path := cfgSnapshot()
			if !enabled || (name == "" && path == "") {
				continue
			}

			online := devices.Online(name, path)
			mw.Synchronize(func() {
				if online != targetOnline {
					targetOnline = online
					applyPauseState()
					if online {
						logging.Event("目标键盘已连接，自动恢复拦截")
					} else {
						logging.Event("目标键盘已断开，自动暂停拦截")
					}
				}
			})
		}
	}()

	// Run the message loop.
	mw.Run()
}

// fatal 在 windowsgui 模式下没有控制台，用弹窗展示启动错误。
func fatal(err error) {
	text, _ := windows.UTF16PtrFromString(err.Error())
	caption, _ := windows.UTF16PtrFromString("keyboard-plus")
	procMessageBoxW.Call(0, uintptr(unsafe.Pointer(text)), uintptr(unsafe.Pointer(caption)), mbOK|mbIconError)
	os.Exit(1)
}
