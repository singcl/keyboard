//go:build windows

// Package autostart 通过 HKCU 的 Run 键管理开机自启。
package autostart

import (
	"os"
	"strings"

	"golang.org/x/sys/windows/registry"
)

const (
	runKeyPath = `Software\Microsoft\Windows\CurrentVersion\Run`
	valueName  = "keyboard"
)

// Enabled 判断当前 exe 是否已设置为开机自启。
func Enabled() bool {
	k, err := registry.OpenKey(registry.CURRENT_USER, runKeyPath, registry.QUERY_VALUE)
	if err != nil {
		return false
	}
	defer k.Close()

	value, _, err := k.GetStringValue(valueName)
	if err != nil || value == "" {
		return false
	}
	exe, err := quotedExePath()
	if err != nil {
		return true
	}
	return strings.EqualFold(value, exe)
}

// Set 开启或关闭开机自启。
func Set(enabled bool) error {
	k, _, err := registry.CreateKey(registry.CURRENT_USER, runKeyPath, registry.SET_VALUE)
	if err != nil {
		return err
	}
	defer k.Close()

	if !enabled {
		if err := k.DeleteValue(valueName); err != nil && err != registry.ErrNotExist {
			return err
		}
		return nil
	}

	exe, err := quotedExePath()
	if err != nil {
		return err
	}
	return k.SetStringValue(valueName, exe)
}

func quotedExePath() (string, error) {
	p, err := os.Executable()
	if err != nil {
		return "", err
	}
	return `"` + p + `"`, nil
}
