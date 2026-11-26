// Package config 负责读写程序配置，配置文件位于 %LOCALAPPDATA%\keyboard-plus\config.json。
package config

import (
	"encoding/json"
	"os"
	"path/filepath"
)

// DefaultDebounceMS 是默认的防抖窗口（毫秒）。
const DefaultDebounceMS = 100

// Config 是持久化的程序配置。
type Config struct {
	DebounceMS int `json:"debounce_ms"`

	// 目标键盘：名称或设备路径匹配任一即可
	TargetName string `json:"target_name"`
	TargetPath string `json:"target_path"`

	// OnlyWhenConnected 为 true 时，仅当目标键盘在线才启用防抖
	OnlyWhenConnected bool `json:"only_when_connected"`

	// Paused 记录用户是否手动暂停了拦截，重启后保持该状态
	Paused bool `json:"paused"`
}

// Path 返回配置文件路径，并确保所在目录存在。
func Path() (string, error) {
	dir, err := os.UserCacheDir()
	if err != nil {
		return "", err
	}
	dir = filepath.Join(dir, "keyboard-plus")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	return filepath.Join(dir, "config.json"), nil
}

// Load 读取配置；文件不存在或内容非法时返回默认值。
func Load() Config {
	cfg := Config{DebounceMS: DefaultDebounceMS}

	path, err := Path()
	if err != nil {
		return cfg
	}
	b, err := os.ReadFile(path)
	if err != nil {
		return cfg
	}
	if err := json.Unmarshal(b, &cfg); err != nil {
		return Config{DebounceMS: DefaultDebounceMS}
	}
	if cfg.DebounceMS < 10 || cfg.DebounceMS > 1000 {
		cfg.DebounceMS = DefaultDebounceMS
	}
	return cfg
}

// Save 写入配置。
func Save(cfg Config) error {
	path, err := Path()
	if err != nil {
		return err
	}
	b, err := json.MarshalIndent(cfg, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(b, '\n'), 0o644)
}
