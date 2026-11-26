//go:build windows

package main

import (
	"strconv"
	"strings"
	"unsafe"

	"github.com/lxn/walk"
	"github.com/lxn/win"
)

// askDebounce 弹出输入框，让用户自定义防抖阈值（10-1000 毫秒）。
//
// 对话框故意不设置 owner：托盘程序的主窗口是隐藏的、没有设置布局，
// 而 walk 在关闭带 owner 的对话框时会重新显示 owner（见 FormBase 的 WM_CLOSE 处理），
// 隐藏主窗口一旦显示就会因为空布局在 startLayout 里 panic；
// windowsgui 模式下 panic 没有控制台可输出，表现就是程序静默退出。
func askDebounce(current int) (int, bool) {
	dlg, err := walk.NewDialog(nil)
	if err != nil {
		return 0, false
	}
	defer dlg.Dispose()

	_ = dlg.SetTitle("自定义防抖阈值")
	dlg.SetLayout(walk.NewVBoxLayout())
	dlg.SetSize(walk.SizeFrom96DPI(walk.Size{Width: 340, Height: 150}, dlg.DPI()))
	centerDialog(dlg)

	label, err := walk.NewLabel(dlg)
	if err != nil {
		return 0, false
	}
	_ = label.SetText("同一颗键在多少毫秒内重新按下视为误触发（10-1000）：")

	edit, err := walk.NewLineEdit(dlg)
	if err != nil {
		return 0, false
	}
	_ = edit.SetText(strconv.Itoa(current))
	_ = edit.SetFocus()

	buttons, err := walk.NewComposite(dlg)
	if err != nil {
		return 0, false
	}
	if err := buttons.SetLayout(walk.NewHBoxLayout()); err != nil {
		return 0, false
	}

	// 按钮使用对话框标准尺寸，并固定在底部
	btnSize := walk.SizeFrom96DPI(walk.Size{Width: 75, Height: 25}, dlg.DPI())
	if err := buttons.SetMinMaxSize(walk.Size{Height: btnSize.Height},
		walk.Size{Width: 16777215, Height: btnSize.Height}); err != nil {
		return 0, false
	}
	if _, err := walk.NewHSpacer(buttons); err != nil {
		return 0, false
	}

	okBtn, err := walk.NewPushButton(buttons)
	if err != nil {
		return 0, false
	}
	_ = okBtn.SetText("确定")
	_ = okBtn.SetMinMaxSize(btnSize, btnSize)

	cancelBtn, err := walk.NewPushButton(buttons)
	if err != nil {
		return 0, false
	}
	_ = cancelBtn.SetText("取消")
	_ = cancelBtn.SetMinMaxSize(btnSize, btnSize)

	value, ok := 0, false
	okBtn.Clicked().Attach(func() {
		ms, err := strconv.Atoi(strings.TrimSpace(edit.Text()))
		if err != nil || ms < 10 || ms > 1000 {
			walk.MsgBox(dlg, "数值无效", "请输入 10-1000 之间的整数。",
				walk.MsgBoxOK|walk.MsgBoxIconWarning)
			_ = edit.SetFocus()
			return
		}
		value, ok = ms, true
		dlg.Accept()
	})
	cancelBtn.Clicked().Attach(func() { dlg.Cancel() })

	_ = dlg.SetDefaultButton(okBtn)
	_ = dlg.SetCancelButton(cancelBtn)

	dlg.Run()
	return value, ok
}

// centerDialog 把对话框放到当前前台窗口所在显示器的中央。
// 没有 owner 时 walk 不会自动居中，这里按显示器工作区自己算。
func centerDialog(dlg *walk.Dialog) {
	size := dlg.SizePixels()
	if size.Width <= 0 || size.Height <= 0 {
		return
	}

	hmon := win.MonitorFromWindow(win.GetForegroundWindow(), win.MONITOR_DEFAULTTONEAREST)
	if hmon == 0 {
		hmon = win.MonitorFromWindow(dlg.Handle(), win.MONITOR_DEFAULTTONEAREST)
	}
	var mi win.MONITORINFO
	mi.CbSize = uint32(unsafe.Sizeof(mi))
	if hmon == 0 || !win.GetMonitorInfo(hmon, &mi) {
		return
	}

	work := mi.RcWork
	x := int(work.Left) + (int(work.Right-work.Left)-size.Width)/2
	y := int(work.Top) + (int(work.Bottom-work.Top)-size.Height)/2
	dlg.SetBoundsPixels(walk.Rectangle{X: x, Y: y, Width: size.Width, Height: size.Height})
}
