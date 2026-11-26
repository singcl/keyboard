//go:build noupdate

package update

import "errors"

// Enabled 表示当前构建是否包含在线检查更新功能。
const Enabled = false

// Latest 在精简版中不可用，仅用于让共用代码通过编译。
func Latest() (Release, error) {
	return Release{}, errors.New("此版本为精简版，不包含检查更新功能")
}
