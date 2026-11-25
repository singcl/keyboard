//go:build darwin

// Package singleinstance 通过文件锁保证同一时间只运行一个程序实例。
package singleinstance

import (
	"os"
	"path/filepath"

	"golang.org/x/sys/unix"
)

// Acquire 尝试获取名为 name 的单实例锁。
// ok 为 false 表示已有实例在运行；否则应在程序退出时调用 release。
func Acquire(name string) (release func(), ok bool) {
	dir, err := os.UserCacheDir()
	if err != nil {
		return func() {}, true
	}
	dir = filepath.Join(dir, "keyboard")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return func() {}, true
	}

	f, err := os.OpenFile(filepath.Join(dir, name+".lock"), os.O_CREATE|os.O_RDWR, 0o644)
	if err != nil {
		// 无法创建锁文件时不阻止启动
		return func() {}, true
	}

	if err := unix.Flock(int(f.Fd()), unix.LOCK_EX|unix.LOCK_NB); err != nil {
		_ = f.Close()
		return func() {}, false
	}

	return func() {
		_ = unix.Flock(int(f.Fd()), unix.LOCK_UN)
		_ = f.Close()
	}, true
}
