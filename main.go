package main

import (
	"bytes"
	_ "embed"
	"fmt"
	"image/png"
	"log"

	"github.com/lxn/walk"
)

//go:embed icon.png
var iconPNG []byte

func main() {
	mw, err := walk.NewMainWindow()
	if err != nil {
		log.Fatal(err)
	}

	img, err := png.Decode(bytes.NewReader(iconPNG))
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

	//ni.SetToolTip("右键选择不同击键或退出")

	for i := 10; i <= 20; i += 5 {
		action := walk.NewAction()
		action.SetText(fmt.Sprintf("击键%d", i))
		action.Triggered().Attach(SetKeyTimesFunc(i))
		ni.ContextMenu().Actions().Add(action)
	}

	// 打开拦截日志
	logAction := walk.NewAction()
	if err := logAction.SetText("打开日志"); err != nil {
		log.Fatal(err)
	}
	logAction.Triggered().Attach(func() {
		if err := openLogFileForUser(); err != nil {
			walk.MsgBox(mw, "打开日志失败", err.Error(), walk.MsgBoxOK|walk.MsgBoxIconError)
		}
	})
	if err := ni.ContextMenu().Actions().Add(logAction); err != nil {
		log.Fatal(err)
	}

	// We put an exit action into the context menu.
	exitAction := walk.NewAction()
	if err := exitAction.SetText("E&xit"); err != nil {
		log.Fatal(err)
	}
	exitAction.Triggered().Attach(func() { walk.App().Exit(0) })
	if err := ni.ContextMenu().Actions().Add(exitAction); err != nil {
		log.Fatal(err)
	}

	ni.SetVisible(true)

	if err := ni.ShowInfo("右键图标选择", "右键图标选择击键（默认10）或退出"); err != nil {
		log.Fatal(err)
	}

	go setupHook()

	// Run the message loop.
	mw.Run()
}

func SetKeyTimesFunc(i int) func() {
	return func() {
		updateKeyPressRate(i)
	}
}
