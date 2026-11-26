package update

import "testing"

func TestNewer(t *testing.T) {
	cases := []struct {
		current, latest string
		want            bool
	}{
		{"1.2.3", "v1.2.4", true},
		{"v1.2.4", "1.2.3", false},
		{"1.2.3", "v1.2.3", false},
		{"1.2", "1.2.0", false},
		{"1.2.0", "1.3", true},
		{"1.9.9", "1.10.0", true},
		{"1.10.0", "1.9.9", false},
		{"dev-abc1234", "v1.2.4", false},
		{"1.2.3", "dev-abc1234", false},
		{"未知", "v1.2.4", false},
	}
	for _, c := range cases {
		if got := Newer(c.current, c.latest); got != c.want {
			t.Errorf("Newer(%q, %q) = %v, want %v", c.current, c.latest, got, c.want)
		}
	}
}

func TestIsVersion(t *testing.T) {
	for _, v := range []string{"1.2.3", "v1.2.3", "1.2", " 1.2.3 "} {
		if !IsVersion(v) {
			t.Errorf("IsVersion(%q) = false, want true", v)
		}
	}
	for _, v := range []string{"", "dev", "dev-abc1234", "1.2.3-beta", "未知"} {
		if IsVersion(v) {
			t.Errorf("IsVersion(%q) = true, want false", v)
		}
	}
}
