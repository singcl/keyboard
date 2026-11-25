package main

import (
	"bytes"
	"io"
	"os"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/lxn/walk"
	"github.com/lxn/win"

	"keyboard/internal/logging"
)

// logTailLines 是日志窗口最多显示的行数，避免大文件拖慢界面。
const logTailLines = 500

var (
	logViewMu   sync.Mutex
	logViewOpen *logView
)

// logView 是内置的日志查看窗口。
type logView struct {
	*walk.MainWindow
	text   *walk.TextEdit
	closed atomic.Bool
}

// showLogView 打开日志窗口；已打开时刷新内容并置于前台。
func showLogView(owner walk.Form) {
	logViewMu.Lock()
	v := logViewOpen
	logViewMu.Unlock()

	if v != nil {
		v.load()
		v.Activate()
		return
	}

	v, err := newLogView()
	if err != nil {
		walk.MsgBox(owner, "打开日志失败", err.Error(), walk.MsgBoxOK|walk.MsgBoxIconError)
		return
	}

	logViewMu.Lock()
	logViewOpen = v
	logViewMu.Unlock()

	v.load()
	v.Show()
	go v.autoRefresh()
}

func newLogView() (*logView, error) {
	mw, err := walk.NewMainWindow()
	if err != nil {
		return nil, err
	}
	v := &logView{MainWindow: mw}
	mw.SetTitle("键盘防误触 · 拦截日志")
	mw.SetSize(walk.Size{Width: 820, Height: 480})

	layout := walk.NewVBoxLayout()
	mw.SetLayout(layout)

	text, err := walk.NewTextEditWithStyle(mw, win.WS_VSCROLL|win.ES_AUTOVSCROLL)
	if err != nil {
		mw.Dispose()
		return nil, err
	}
	if err := text.SetReadOnly(true); err != nil {
		mw.Dispose()
		return nil, err
	}
	v.text = text
	if err := layout.SetStretchFactor(text, 1); err != nil {
		mw.Dispose()
		return nil, err
	}

	buttons, err := walk.NewComposite(mw)
	if err != nil {
		mw.Dispose()
		return nil, err
	}
	buttonLayout := walk.NewHBoxLayout()
	_ = buttonLayout.SetMargins(walk.Margins{})
	buttons.SetLayout(buttonLayout)

	// 按钮使用对话框标准尺寸，不随窗口拉伸
	btnSize := walk.SizeFrom96DPI(walk.Size{Width: 75, Height: 25}, mw.DPI())
	if err := buttons.SetMinMaxSize(walk.Size{Height: btnSize.Height},
		walk.Size{Width: 16777215, Height: btnSize.Height}); err != nil {
		mw.Dispose()
		return nil, err
	}

	// 左侧放一个可伸展的空白占位，把按钮紧凑地推到右下角
	if _, err := walk.NewHSpacer(buttons); err != nil {
		mw.Dispose()
		return nil, err
	}

	addButton := func(title string, onClick func()) {
		b, err := walk.NewPushButton(buttons)
		if err != nil {
			return
		}
		if err := b.SetMinMaxSize(btnSize, btnSize); err != nil {
			return
		}
		b.SetText(title)
		b.Clicked().Attach(onClick)
	}
	addButton("刷新", v.load)
	addButton("清空", func() {
		if walk.DlgCmdNo == walk.MsgBox(v, "清空日志", "确定要清空拦截日志吗？",
			walk.MsgBoxYesNo|walk.MsgBoxIconQuestion) {
			return
		}
		if err := logging.Clear(); err != nil {
			walk.MsgBox(v, "清空日志失败", err.Error(), walk.MsgBoxOK|walk.MsgBoxIconError)
			return
		}
		v.load()
	})
	addButton("关闭", func() { _ = v.Close() })

	mw.Closing().Attach(func(canceled *bool, reason walk.CloseReason) {
		v.closed.Store(true)
		logViewMu.Lock()
		if logViewOpen == v {
			logViewOpen = nil
		}
		logViewMu.Unlock()
	})

	return v, nil
}

// load 读取日志尾部并显示，内容未变化时不重设文本以保留滚动位置。
func (v *logView) load() {
	text := tailLog(logTailLines)
	if v.text.Text() == text {
		return
	}
	_ = v.text.SetText(text)
	v.text.SetTextSelection(len(text), len(text))
}

// autoRefresh 每 2 秒刷新一次，窗口关闭后自动退出。
func (v *logView) autoRefresh() {
	ticker := time.NewTicker(2 * time.Second)
	defer ticker.Stop()

	for range ticker.C {
		if v.closed.Load() {
			return
		}
		v.Synchronize(func() {
			if !v.closed.Load() {
				v.load()
			}
		})
	}
}

// tailLog 只从文件尾部按块读取，返回最后 maxLines 行；文件不存在或为空时给出提示。
func tailLog(maxLines int) string {
	path, err := logging.Path()
	if err != nil {
		return "（无法定位日志文件：" + err.Error() + "）"
	}
	f, err := os.Open(path)
	if err != nil {
		return "（暂无日志）"
	}
	defer f.Close()

	info, err := f.Stat()
	if err != nil || info.Size() == 0 {
		return "（暂无日志）"
	}

	const chunkSize = 32 * 1024
	var (
		data      []byte
		offset    = info.Size()
		truncated bool
	)
	for offset > 0 {
		n := int64(chunkSize)
		if offset < n {
			n = offset
		}
		offset -= n

		buf := make([]byte, n)
		if _, err := f.ReadAt(buf, offset); err != nil && err != io.EOF {
			return "（读取日志失败：" + err.Error() + "）"
		}
		data = append(buf, data...)

		if bytes.Count(data, []byte{'\n'}) > maxLines {
			break
		}
	}
	truncated = offset > 0

	// 未读到文件开头时，首行可能是被截断的半行，丢弃它
	if truncated {
		i := bytes.IndexByte(data, '\n')
		if i < 0 {
			return "（暂无日志）"
		}
		data = data[i+1:]
	}

	content := strings.TrimRight(string(data), "\r\n")
	if content == "" {
		return "（暂无日志）"
	}
	lines := strings.Split(content, "\n")
	if len(lines) > maxLines {
		lines = lines[len(lines)-maxLines:]
	}
	return strings.Join(lines, "\r\n")
}
