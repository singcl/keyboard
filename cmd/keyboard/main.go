package main

import (
	"bytes"
	"fmt"
	"image/png"
	"log"

	"github.com/lxn/walk"

	"keyboard/internal/assets"
	"keyboard/internal/hook"
	"keyboard/internal/logging"
)

func main() {
	mw, err := walk.NewMainWindow()
	if err != nil {
		log.Fatal(err)
	}

	img, err := png.Decode(bytes.NewReader(assets.IconPNG))
	if err != nil {
		log.Fatal(err)
	}
	icon, err := walk.NewIconFromImage(img)
	if err != nil {
		log.Fatal(err)
	}

	ni, err := walk.NewNotifyIcon(mw)
	if err != nil {
		log.Fatal(err)
	}

	defer ni.Dispose()

	if err := ni.SetIcon(icon); err != nil {
		log.Fatal(err)
	}

	//ni.SetToolTip("右键设置拦截阈值或退出")

	// 同一颗键在 ms 毫秒内被重新按下视为误触发并拦截
	for _, item := range []struct {
		rate int
		ms   int
	}{
		{10, 100},
		{15, 67},
		{20, 50},
	} {
		action := walk.NewAction()
		action.SetText(fmt.Sprintf("防抖 %dms", item.ms))
		action.Triggered().Attach(setKeyTimesFunc(item.rate))
		ni.ContextMenu().Actions().Add(action)
	}

	// 打开拦截日志
	logAction := walk.NewAction()
	if err := logAction.SetText("打开日志"); err != nil {
		log.Fatal(err)
	}
	logAction.Triggered().Attach(func() {
		if err := logging.Open(); err != nil {
			walk.MsgBox(mw, "打开日志失败", err.Error(), walk.MsgBoxOK|walk.MsgBoxIconError)
		}
	})
	if err := ni.ContextMenu().Actions().Add(logAction); err != nil {
		log.Fatal(err)
	}

	// We put an exit action into the context menu.
	exitAction := walk.NewAction()
	if err := exitAction.SetText("退出"); err != nil {
		log.Fatal(err)
	}
	exitAction.Triggered().Attach(func() { walk.App().Exit(0) })
	if err := ni.ContextMenu().Actions().Add(exitAction); err != nil {
		log.Fatal(err)
	}

	ni.SetVisible(true)

	if err := ni.ShowInfo("右键图标设置", "右键图标可设置拦截阈值（默认 100ms）或退出"); err != nil {
		log.Fatal(err)
	}

	go hook.Setup(logging.Blocked)

	// Run the message loop.
	mw.Run()
}

func setKeyTimesFunc(i int) func() {
	return func() {
		hook.SetRate(i)
	}
}
