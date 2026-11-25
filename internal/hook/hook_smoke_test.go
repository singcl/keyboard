package hook

import (
	"testing"
	"time"
	"unsafe"
)

func callKey(msg uintptr, vk uint32) {
	kb := kbdLLHookStruct{VkCode: vk, ScanCode: 0x1E}
	keyboardProc(0, msg, uintptr(unsafe.Pointer(&kb)))
}

func TestPauseAndBlock(t *testing.T) {
	blocked := 0
	onBlock = func(uint32, uint32, bool, time.Duration) { blocked++ }
	SetDebounce(100 * time.Millisecond)
	lastDown = time.Time{}
	keyDown = [256]bool{}

	// 暂停期间不应拦截，也不应记录
	SetPaused(true)
	callKey(wmKeyDown, 0x41)
	callKey(wmKeyUp, 0x41)
	callKey(wmKeyDown, 0x41)
	if blocked != 0 {
		t.Fatalf("paused: blocked=%d, want 0", blocked)
	}

	// 恢复后，同一按键快速重按应被拦截
	SetPaused(false)
	callKey(wmKeyDown, 0x41)
	callKey(wmKeyUp, 0x41)
	callKey(wmKeyDown, 0x41)
	if blocked != 1 {
		t.Fatalf("resumed: blocked=%d, want 1", blocked)
	}
	callKey(wmKeyUp, 0x41)
}

func TestShouldBlock(t *testing.T) {
	const debounce = 100 * time.Millisecond

	cases := []struct {
		name       string
		autoRepeat bool
		vk, prev   uint32
		since      time.Duration
		want       bool
	}{
		{"自动重复放行", true, 'A', 'A', 10 * time.Millisecond, false},
		{"首次按下放行", false, 'A', 0, 0, false},
		{"不同按键放行", false, 'B', 'A', 10 * time.Millisecond, false},
		{"快速重按拦截", false, 'A', 'A', 50 * time.Millisecond, true},
		{"超过窗口放行", false, 'A', 'A', 150 * time.Millisecond, false},
		{"等于窗口放行", false, 'A', 'A', debounce, false},
	}

	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := ShouldBlock(c.autoRepeat, c.vk, c.prev, c.since, debounce); got != c.want {
				t.Fatalf("ShouldBlock() = %v, want %v", got, c.want)
			}
		})
	}
}
