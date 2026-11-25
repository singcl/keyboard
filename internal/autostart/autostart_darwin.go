//go:build darwin

// Package autostart 通过 LaunchAgent 管理开机自启。
package autostart

import (
	"fmt"
	"html"
	"os"
	"path/filepath"
	"strings"
)

const label = "com.singcl.keyboard"

const plistTemplate = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>%s</string>
	<key>ProgramArguments</key>
	<array>
		<string>%s</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>ProcessType</key>
	<string>Interactive</string>
</dict>
</plist>
`

// plistPath 返回 ~/Library/LaunchAgents/com.singcl.keyboard.plist，并确保目录存在。
func plistPath() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	dir := filepath.Join(home, "Library", "LaunchAgents")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	return filepath.Join(dir, label+".plist"), nil
}

// Enabled 判断当前 exe 是否已写入开机自启配置。
func Enabled() bool {
	path, err := plistPath()
	if err != nil {
		return false
	}
	b, err := os.ReadFile(path)
	if err != nil {
		return false
	}
	exe, err := os.Executable()
	if err != nil {
		return true
	}
	return strings.Contains(string(b), html.EscapeString(exe))
}

// Set 开启或关闭开机自启；写入的 LaunchAgent 在下次登录时生效。
func Set(enabled bool) error {
	path, err := plistPath()
	if err != nil {
		return err
	}

	if !enabled {
		if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
			return err
		}
		return nil
	}

	exe, err := os.Executable()
	if err != nil {
		return err
	}
	content := fmt.Sprintf(plistTemplate, label, html.EscapeString(exe))
	return os.WriteFile(path, []byte(content), 0o644)
}
