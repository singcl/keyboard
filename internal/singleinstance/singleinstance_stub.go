//go:build !windows && !darwin

// Package singleinstance 保证同一时间只运行一个程序实例，具体实现按平台拆分。
package singleinstance

// Acquire 在非 Windows 平台暂未实现单实例锁，直接放行。
func Acquire(name string) (release func(), ok bool) {
	return func() {}, true
}
