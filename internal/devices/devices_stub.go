//go:build !windows

package devices

// List 在非 Windows 平台暂未实现设备枚举。
func List() ([]Device, error) {
	return nil, ErrUnsupported
}
