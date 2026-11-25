package devices

import (
	"strings"
	"testing"
)

func TestParseIDs(t *testing.T) {
	cases := []struct {
		path     string
		vid, pid string
	}{
		{`\\?\HID#VID_046D&PID_C31C&MI_00#8&2f6a1c3&0&0000#{884b96c3-56ef-11d1-bc8c-00a0c91405dd}`, "046D", "C31C"},
		{`\\?\HID#VID_3434&PID_0280#7&1a2b3c4d&0&0000#{...}`, "3434", "0280"},
		{`\\?\HID#{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&02046d_PID&b34c_REV&0014_b02a6f00d815&Col01#9&128bb436&0&0000#{884b96c3-56ef-11d1-bc8c-00a0c91405dd}`, "046D", "B34C"},
		{`ACPI\PNP0303\4&2f6a1c3&0`, "", ""},
	}
	for _, c := range cases {
		vid, pid := parseIDs(c.path)
		if vid != c.vid || pid != c.pid {
			t.Errorf("parseIDs(%q) = %q/%q, want %q/%q", c.path, vid, pid, c.vid, c.pid)
		}
	}
}

func TestMatches(t *testing.T) {
	d := Device{Name: "LANGTU BT5.0_3", Path: `\\?\HID#VID_046D&PID_B34C`}
	if !d.Matches("LANGTU BT5.0_3", "") {
		t.Error("按名称应匹配")
	}
	if !d.Matches("", `\\?\HID#VID_046D&PID_B34C`) {
		t.Error("按路径应匹配")
	}
	if d.Matches("其他键盘", "") {
		t.Error("不同名称不应匹配")
	}
}

func TestFormatList(t *testing.T) {
	list := []Device{
		{Name: "AULA-F99 5.0", VID: "3554", PID: "FA07"},
		{Name: "AULA-F99 5.0", VID: "3554", PID: "FA07"},
		{Name: "LANGTU BT5.0_3", VID: "046D", PID: "B34C"},
		{Name: "ACPI HPQ8002"},
	}

	got := FormatList(list, "LANGTU BT5.0_3", "")
	if n := strings.Count(got, "AULA-F99 5.0"); n != 1 {
		t.Errorf("重复设备应去重，出现 %d 次", n)
	}
	if !strings.Contains(got, "实体 2") {
		t.Errorf("实体设备统计错误：\n%s", got)
	}
	if !strings.Contains(got, "LANGTU BT5.0_3（当前目标）") {
		t.Errorf("目标标记缺失：\n%s", got)
	}
	if !strings.Contains(got, "系统 / 虚拟设备") {
		t.Errorf("缺少系统设备分组：\n%s", got)
	}
}

func TestList(t *testing.T) {
	list, err := List()
	if err != nil {
		t.Fatal(err)
	}
	for _, d := range list {
		t.Logf("%q brand=%q vid=%s pid=%s path=%s", d.DisplayName(), d.Brand, d.VID, d.PID, d.Path)
	}
}
