// Package devices 通过 Raw Input 枚举系统键盘设备，用于展示品牌/型号信息。
//
// 注意：低级键盘钩子拿不到设备信息，因此这里只能识别设备，不能按设备拦截。
package devices

import (
	"strings"
	"unsafe"

	"golang.org/x/sys/windows"
)

const (
	rimTypeKeyboard = 1

	ridiDeviceName = 0x20000007

	devPropTypeString = 0x00000012
	crBufferSmall     = 0x1A

	cmLocateDevNodeNormal = 0
	maxParentDepth        = 5
)

var (
	user32   = windows.NewLazySystemDLL("user32.dll")
	hidDLL   = windows.NewLazySystemDLL("hid.dll")
	cfgmgr32 = windows.NewLazySystemDLL("cfgmgr32.dll")

	procGetRawInputDeviceList        = user32.NewProc("GetRawInputDeviceList")
	procGetRawInputDeviceInfo        = user32.NewProc("GetRawInputDeviceInfoW")
	procHidDGetProductString         = hidDLL.NewProc("HidD_GetProductString")
	procCMGetDeviceInterfaceProperty = cfgmgr32.NewProc("CM_Get_Device_Interface_PropertyW")
	procCMLocateDevNode              = cfgmgr32.NewProc("CM_Locate_DevNodeW")
	procCMGetParent                  = cfgmgr32.NewProc("CM_Get_Parent")
	procCMGetDevNodeProperty         = cfgmgr32.NewProc("CM_Get_DevNode_PropertyW")

	// DEVPKEY_Device_FriendlyName
	devPropKeyFriendlyName = devPropKey{
		FmtID: windows.GUID{
			Data1: 0xa45c254e,
			Data2: 0xdf1c,
			Data3: 0x4efd,
			Data4: [8]byte{0x80, 0x20, 0x67, 0xd1, 0x46, 0xa8, 0x50, 0xe0},
		},
		PID: 14,
	}

	// DEVPKEY_Device_InstanceId
	devPropKeyInstanceID = devPropKey{
		FmtID: windows.GUID{
			Data1: 0x78c34fc8,
			Data2: 0x104a,
			Data3: 0x4aca,
			Data4: [8]byte{0x9e, 0xa4, 0x52, 0x4d, 0x52, 0x99, 0x6e, 0x57},
		},
		PID: 256,
	}

	// DEVPKEY_Device_BusReportedDeviceDesc
	devPropKeyBusReportedDesc = devPropKey{
		FmtID: windows.GUID{
			Data1: 0x540b947e,
			Data2: 0x8b40,
			Data3: 0x45bc,
			Data4: [8]byte{0xa8, 0xa2, 0x6a, 0x0b, 0x89, 0x4c, 0xbd, 0xa2},
		},
		PID: 4,
	}

	// 这些名称过于笼统，不能代表具体型号
	genericNames = []string{
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

	vendors = map[string]string{
		"03F0": "HP",
		"045E": "Microsoft",
		"046A": "CHERRY",
		"046D": "Logitech",
		"04D9": "Holtek",
		"05AC": "Apple",
		"0853": "Topre",
		"0B05": "ASUS",
		"1532": "Razer",
		"17EF": "Lenovo",
		"1B1C": "Corsair",
		"258A": "SinoWealth",
		"3434": "Keychron",
		"413C": "Dell",
	}
)

type devPropKey struct {
	FmtID windows.GUID
	PID   uint32
}

type rawInputDeviceList struct {
	Device windows.Handle
	Type   uint32
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
		return d.Name
	}
	if d.Brand != "" {
		return d.Brand + " 键盘"
	}
	return shortName(d.Path)
}

// Matches 判断设备是否与指定目标匹配：路径或名称一致即可。
func (d Device) Matches(name, path string) bool {
	if path != "" && d.Path == path {
		return true
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

// List 枚举当前系统中的键盘设备。
func List() ([]Device, error) {
	var count uint32
	r, _, err := procGetRawInputDeviceList.Call(0,
		uintptr(unsafe.Pointer(&count)), unsafe.Sizeof(rawInputDeviceList{}))
	if r == ^uintptr(0) {
		return nil, err
	}
	if count == 0 {
		return nil, nil
	}

	list := make([]rawInputDeviceList, count)
	r, _, err = procGetRawInputDeviceList.Call(uintptr(unsafe.Pointer(&list[0])),
		uintptr(unsafe.Pointer(&count)), unsafe.Sizeof(rawInputDeviceList{}))
	if r == ^uintptr(0) {
		return nil, err
	}

	result := make([]Device, 0, count)
	for _, item := range list[:count] {
		if item.Type != rimTypeKeyboard {
			continue
		}
		path, err := deviceName(item.Device)
		if err != nil || path == "" {
			continue
		}
		name := friendlyName(path)
		if name == "" || isGenericName(name) {
			if p := productName(path); p != "" && !isGenericName(p) {
				name = p
			} else {
				name = ""
			}
		}
		vid, pid := parseIDs(path)
		result = append(result, Device{
			Name:  name,
			Brand: vendors[vid],
			VID:   vid,
			PID:   pid,
			Path:  path,
		})
	}
	return result, nil
}

func deviceName(h windows.Handle) (string, error) {
	var size uint32
	_, _, err := procGetRawInputDeviceInfo.Call(uintptr(h), ridiDeviceName, 0,
		uintptr(unsafe.Pointer(&size)))
	if size == 0 {
		return "", err
	}

	buf := make([]uint16, size)
	r, _, err := procGetRawInputDeviceInfo.Call(uintptr(h), ridiDeviceName,
		uintptr(unsafe.Pointer(&buf[0])), uintptr(unsafe.Pointer(&size)))
	if r == ^uintptr(0) {
		return "", err
	}
	return windows.UTF16ToString(buf), nil
}

// friendlyName 依次从接口、设备节点及其父节点读取名称，优先返回具体型号名。
func friendlyName(path string) string {
	var candidates []string

	if n := interfaceStringProp(path, &devPropKeyFriendlyName); n != "" {
		candidates = append(candidates, n)
	}

	instanceID := interfaceStringProp(path, &devPropKeyInstanceID)
	if devInst, ok := locateDevNode(instanceID); ok {
		for depth := 0; depth < maxParentDepth; depth++ {
			if n := devNodeStringProp(devInst, &devPropKeyBusReportedDesc); n != "" {
				candidates = append(candidates, n)
			}
			if n := devNodeStringProp(devInst, &devPropKeyFriendlyName); n != "" {
				candidates = append(candidates, n)
			}
			parent, ok := parentDevNode(devInst)
			if !ok {
				break
			}
			devInst = parent
		}
	}

	for _, n := range candidates {
		if !isGenericName(n) {
			return n
		}
	}
	if len(candidates) > 0 {
		return candidates[0]
	}
	return ""
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

func locateDevNode(instanceID string) (uint32, bool) {
	if instanceID == "" {
		return 0, false
	}
	ptr, err := windows.UTF16PtrFromString(instanceID)
	if err != nil {
		return 0, false
	}

	var devInst uint32
	r, _, _ := procCMLocateDevNode.Call(uintptr(unsafe.Pointer(&devInst)),
		uintptr(unsafe.Pointer(ptr)), cmLocateDevNodeNormal)
	if r != 0 {
		return 0, false
	}
	return devInst, true
}

func parentDevNode(devInst uint32) (uint32, bool) {
	var parent uint32
	r, _, _ := procCMGetParent.Call(uintptr(unsafe.Pointer(&parent)), uintptr(devInst), 0)
	if r != 0 {
		return 0, false
	}
	return parent, true
}

func interfaceStringProp(path string, key *devPropKey) string {
	ptr, err := windows.UTF16PtrFromString(path)
	if err != nil {
		return ""
	}
	return readStringProp(func(buf, size, propType uintptr) uintptr {
		r, _, _ := procCMGetDeviceInterfaceProperty.Call(
			uintptr(unsafe.Pointer(ptr)),
			uintptr(unsafe.Pointer(key)),
			propType,
			buf,
			size,
			0,
		)
		return r
	})
}

func devNodeStringProp(devInst uint32, key *devPropKey) string {
	return readStringProp(func(buf, size, propType uintptr) uintptr {
		r, _, _ := procCMGetDevNodeProperty.Call(
			uintptr(devInst),
			uintptr(unsafe.Pointer(key)),
			propType,
			buf,
			size,
			0,
		)
		return r
	})
}

// readStringProp 调用一次查询取长度，再取内容。
func readStringProp(query func(buf, size, propType uintptr) uintptr) string {
	var propType, size uint32
	r := query(0, uintptr(unsafe.Pointer(&size)), uintptr(unsafe.Pointer(&propType)))
	if r != 0 && r != crBufferSmall {
		return ""
	}
	if size < 2 {
		return ""
	}

	buf := make([]byte, size)
	r = query(uintptr(unsafe.Pointer(&buf[0])),
		uintptr(unsafe.Pointer(&size)), uintptr(unsafe.Pointer(&propType)))
	if r != 0 || propType != devPropTypeString {
		return ""
	}
	return windows.UTF16ToString(unsafe.Slice((*uint16)(unsafe.Pointer(&buf[0])), len(buf)/2))
}

// productName 通过 HID 接口读取产品名，失败时返回空字符串。
func productName(path string) string {
	ptr, err := windows.UTF16PtrFromString(path)
	if err != nil {
		return ""
	}

	h, err := windows.CreateFile(ptr, 0, windows.FILE_SHARE_READ|windows.FILE_SHARE_WRITE,
		nil, windows.OPEN_EXISTING, 0, 0)
	if err != nil {
		return ""
	}
	defer windows.CloseHandle(h)

	var buf [128]uint16
	r, _, _ := procHidDGetProductString.Call(uintptr(h),
		uintptr(unsafe.Pointer(&buf[0])), uintptr(len(buf)*2))
	if r == 0 {
		return ""
	}
	return windows.UTF16ToString(buf[:])
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
