//go:build !windows && !darwin

package logging

// keyName 在当前平台暂未实现按键名称解析，返回空字符串。
func keyName(vk, scanCode uint32, extended bool) string {
	return ""
}
