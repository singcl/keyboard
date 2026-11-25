package logging

import (
	"os"
	"strings"
	"testing"
	"time"
)

func TestAsyncWrite(t *testing.T) {
	t.Setenv("LOCALAPPDATA", t.TempDir())

	Blocked(0x41, 0x1E, false, 3200*time.Microsecond)

	path, err := Path()
	if err != nil {
		t.Fatal(err)
	}

	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if b, err := os.ReadFile(path); err == nil && strings.Contains(string(b), "vk=0x41") {
			t.Logf("log content: %s", strings.TrimSpace(string(b)))
			return
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatal("log entry not written")
}
