// Package devices 枚举系统键盘设备，用于展示品牌/型号信息。
//
// 注意：低级键盘钩子拿不到设备信息，因此这里只能识别设备，不能按设备拦截。
// 枚举能力由各平台实现提供：Windows 见 devices_windows.go（Raw Input），
// 其他平台目前为占位实现；设备分组、文本整理等逻辑是平台无关的。
package devices

import (
	"errors"
	"fmt"
	"sort"
	"strings"
)

// ErrUnsupported 表示当前平台尚未实现设备枚举。
var ErrUnsupported = errors.New("devices: 当前平台暂不支持设备枚举")

// 这些名称过于笼统，不能代表具体型号
var genericNames = []string{
	"hid-compliant",
	"符合 hid",
	"hid keyboard",
	"hid 键盘",
	"bluetooth le",
	"蓝牙 le",
	"usb 输入设备",
	"usb input device",
	"usb composite device",
	"usb 复合设备",
	"pci to isa",
	"pci 到 isa 桥",
}

// Device 是一个键盘设备。
type Device struct {
	Name  string // 设备友好名或 HID 产品名，可能为空
	Brand string // 根据 VID 推断的品牌，可能为空
	VID   string // USB 厂商 ID，如 046D
	PID   string // USB 产品 ID
	Path  string // 设备接口路径
}

// DisplayName 返回用于展示的名称：友好名 > 品牌 > 路径推断。
func (d Device) DisplayName() string {
	if d.Name != "" {
		return strings.TrimSpace(d.Name)
	}
	if d.Brand != "" {
		return d.Brand + " 键盘"
	}
	return shortName(d.Path)
}

// Matches 判断设备是否与指定目标匹配。
// 配置了路径时按路径精确匹配：同型号的多把键盘名称相同，只有路径能区分；
// 没有路径时才退回到按名称匹配。
func (d Device) Matches(name, path string) bool {
	if path != "" {
		return d.Path == path
	}
	return name != "" && d.Name == name
}

// Online 判断目标键盘当前是否在线；name 与 path 任一为空则不参与匹配。
func Online(name, path string) bool {
	list, err := List()
	if err != nil {
		return false
	}
	for _, d := range list {
		if d.Matches(name, path) {
			return true
		}
	}
	return false
}

// Group 去重后按实体键盘与系统/虚拟设备分组，组内按展示名排序。
func Group(list []Device) (physical, system []Device) {
	physical = make([]Device, 0, len(list))
	system = make([]Device, 0, len(list))
	seen := make(map[string]bool, len(list))
	for _, d := range list {
		// 同一把键盘可能有多个采集接口，按展示名+VID/PID 去重
		key := d.DisplayName() + "|" + d.VID + "|" + d.PID
		if seen[key] {
			continue
		}
		seen[key] = true

		if d.VID != "" && d.PID != "" {
			physical = append(physical, d)
		} else {
			system = append(system, d)
		}
	}

	byName := func(items []Device) {
		sort.SliceStable(items, func(i, j int) bool {
			return strings.ToLower(items[i].DisplayName()) < strings.ToLower(items[j].DisplayName())
		})
	}
	byName(physical)
	byName(system)
	return physical, system
}

// Labels 返回一组设备各自用于展示的名称。
// 名称重复的设备追加 VID/PID，避免菜单里出现多条一模一样的条目。
func Labels(list []Device) []string {
	counts := make(map[string]int, len(list))
	for _, d := range list {
		counts[d.DisplayName()]++
	}

	labels := make([]string, len(list))
	for i, d := range list {
		label := d.DisplayName()
		if counts[label] > 1 && (d.VID != "" || d.PID != "") {
			label = fmt.Sprintf("%s（VID_%s / PID_%s）", label, d.VID, d.PID)
		}
		labels[i] = label
	}
	return labels
}

// FormatList 把设备列表整理成便于阅读的分组文本：实体键盘在前，系统/虚拟设备在后。
func FormatList(list []Device, targetName, targetPath string) string {
	physical, system := Group(list)

	var b strings.Builder
	fmt.Fprintf(&b, "检测到键盘设备 %d 个（实体 %d，系统/虚拟 %d）。",
		len(physical)+len(system), len(physical), len(system))

	writeGroup := func(title string, items []Device) {
		if len(items) == 0 {
			return
		}
		fmt.Fprintf(&b, "\n\n%s\n", title)
		labels := Labels(items)
		for i, d := range items {
			fmt.Fprintf(&b, "  %d. %s", i+1, strings.TrimSpace(labels[i]))
			if d.Matches(targetName, targetPath) {
				b.WriteString("（当前目标）")
			}
			if d.VID != "" || d.PID != "" {
				fmt.Fprintf(&b, "\n     VID_%s / PID_%s", d.VID, d.PID)
			}
			if i != len(items)-1 {
				b.WriteString("\n")
			}
		}
	}
	writeGroup("实体键盘", physical)
	writeGroup("系统 / 虚拟设备（通常不是外接键盘）", system)

	b.WriteString("\n\n提示：拦截对所有键盘生效，指定目标只影响是否自动暂停。")
	return b.String()
}

func isGenericName(name string) bool {
	lower := strings.ToLower(name)
	for _, g := range genericNames {
		if strings.Contains(lower, g) {
			return true
		}
	}
	return false
}

// parseIDs 从设备路径中解析出 VID/PID，兼容 VID_046D 与 BLE 的 VID&02046D 形式。
func parseIDs(path string) (vid, pid string) {
	upper := strings.ToUpper(path)
	return parseID(upper, "VID"), parseID(upper, "PID")
}

func parseID(s, kind string) string {
	if i := strings.Index(s, kind+"_"); i >= 0 {
		return firstHex4(s[i+len(kind)+1:])
	}

	i := strings.Index(s, kind+"&")
	if i < 0 {
		return ""
	}
	rest := s[i+len(kind)+1:]
	// 蓝牙 LE 设备的路径形如 VID&02046D，前两位是固定前缀
	if len(rest) >= 6 && rest[0:2] == "02" {
		if v := firstHex4(rest[2:]); v != "" {
			return v
		}
	}
	return firstHex4(rest)
}

func firstHex4(s string) string {
	if len(s) < 4 {
		return ""
	}
	for _, c := range s[:4] {
		if !(c >= '0' && c <= '9' || c >= 'A' && c <= 'F') {
			return ""
		}
	}
	return s[:4]
}

// shortName 从设备路径中提取一个可读的简短名称。
func shortName(path string) string {
	if i := strings.Index(path, "HID#"); i >= 0 {
		rest := path[i+len("HID#"):]
		if j := strings.Index(rest, "#"); j >= 0 {
			rest = rest[:j]
		}
		if strings.HasPrefix(rest, "{") {
			return "蓝牙 HID 设备"
		}
		if j := strings.Index(rest, "&"); j >= 0 {
			rest = rest[:j]
		}
		return rest
	}
	if i := strings.Index(path, "ACPI#"); i >= 0 {
		rest := path[i+len("ACPI#"):]
		if j := strings.Index(rest, "#"); j >= 0 {
			rest = rest[:j]
		}
		return "ACPI " + rest
	}
	return "键盘设备"
}
