// Package update 通过 GitHub Releases 检查是否有新版本。
//
// 只做「检查 + 提示」，不下载、不替换程序：两个平台的产物都未签名，
// 自动替换会触发 SmartScreen / Gatekeeper，macOS 替换后还可能丢失输入监控授权。
//
// 带 noupdate 构建标签编译时（精简版）不链接 net/http，体积约为一半，
// 此时 Enabled 为 false，界面不显示「检查更新」。
package update

import (
	"strconv"
	"strings"
)

// Release 是最新发布的信息。
type Release struct {
	Tag string // 例如 v1.2.4
	URL string // 发布页面地址
}

// Newer 判断 latest 是否比 current 新；任一版本号无法解析时返回 false。
func Newer(current, latest string) bool {
	c, ok := parse(current)
	if !ok {
		return false
	}
	l, ok := parse(latest)
	if !ok {
		return false
	}
	for i := 0; i < len(c) || i < len(l); i++ {
		var a, b int
		if i < len(c) {
			a = c[i]
		}
		if i < len(l) {
			b = l[i]
		}
		if a != b {
			return b > a
		}
	}
	return false
}

// IsVersion 判断字符串是否为可比较的版本号，如 1.2.3 或 v1.2.3。
// 开发构建（dev-abc1234）等无法解析的版本号返回 false。
func IsVersion(v string) bool {
	_, ok := parse(v)
	return ok
}

// parse 把 1.2.3 / v1.2.3 解析成数字切片；带前后缀等无法解析时返回 false。
func parse(v string) ([]int, bool) {
	v = strings.TrimPrefix(strings.TrimSpace(v), "v")
	if v == "" {
		return nil, false
	}

	parts := strings.Split(v, ".")
	nums := make([]int, 0, len(parts))
	for _, p := range parts {
		n, err := strconv.Atoi(p)
		if err != nil || n < 0 {
			return nil, false
		}
		nums = append(nums, n)
	}
	return nums, true
}
