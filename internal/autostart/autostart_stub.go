//go:build !windows && !darwin

// Package autostart 管理开机自启，具体实现按平台拆分。
package autostart

import "errors"

// ErrUnsupported 表示当前平台尚未实现开机自启。
var ErrUnsupported = errors.New("autostart: 当前平台暂不支持开机自启")

// Enabled 在非 Windows 平台恒为 false。
func Enabled() bool { return false }

// Set 在非 Windows 平台暂未实现。
func Set(enabled bool) error { return ErrUnsupported }
