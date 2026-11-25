//go:build !windows

package logging

// keyName 在非 Windows 平台暂未实现按键名称解析，返回空字符串。
func keyName(scanCode uint32, extended bool) string {
	return ""
}
