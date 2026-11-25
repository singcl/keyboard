//go:build !windows

package hook

import (
	"log"
	"time"
)

// Setup 在非 Windows 平台暂未实现全局键盘拦截，直接返回。
func Setup(handler BlockHandler) {
	if handler == nil {
		handler = func(uint32, uint32, bool, time.Duration) {}
	}
	onBlock = handler
	log.Println("hook: 当前平台暂不支持全局键盘拦截，防抖功能未生效")
}
