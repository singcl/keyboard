package config

import (
	"os"
	"testing"
)

func TestSaveLoad(t *testing.T) {
	t.Setenv("LOCALAPPDATA", t.TempDir())

	if got := Load().DebounceMS; got != DefaultDebounceMS {
		t.Fatalf("missing file: got %d, want %d", got, DefaultDebounceMS)
	}

	if err := Save(Config{DebounceMS: 67}); err != nil {
		t.Fatal(err)
	}
	if got := Load().DebounceMS; got != 67 {
		t.Fatalf("roundtrip: got %d, want 67", got)
	}

	path, err := Path()
	if err != nil {
		t.Fatal(err)
	}

	if err := os.WriteFile(path, []byte("not json"), 0o644); err != nil {
		t.Fatal(err)
	}
	if got := Load().DebounceMS; got != DefaultDebounceMS {
		t.Fatalf("invalid json: got %d, want %d", got, DefaultDebounceMS)
	}

	if err := os.WriteFile(path, []byte(`{"debounce_ms": 5}`), 0o644); err != nil {
		t.Fatal(err)
	}
	if got := Load().DebounceMS; got != DefaultDebounceMS {
		t.Fatalf("out of range: got %d, want %d", got, DefaultDebounceMS)
	}
}
