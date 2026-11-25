//go:build windows

package main

import (
	"strconv"
	"strings"

	"github.com/lxn/walk"
)

// askDebounce 弹出输入框，让用户自定义防抖阈值（10-1000 毫秒）。
func askDebounce(owner walk.Form, current int) (int, bool) {
	dlg, err := walk.NewDialog(owner)
	if err != nil {
		return 0, false
	}
	defer dlg.Dispose()

	_ = dlg.SetTitle("自定义防抖阈值")
	dlg.SetLayout(walk.NewVBoxLayout())
	dlg.SetSize(walk.SizeFrom96DPI(walk.Size{Width: 340, Height: 150}, dlg.DPI()))

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
