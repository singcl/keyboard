// Rust 与 Objective-C / C 之间的回调约定。
//
// 该头文件取代 Go 版 cgo 的 _cgo_export.h：函数由 Rust 侧 #[no_mangle] 导出，
// cocoa_darwin.m 与 hook_darwin.c 通过这里声明的原型调用。
#ifndef BRIDGE_H
#define BRIDGE_H

#include <stdint.h>

// 菜单栏状态：0 运行中、1 已暂停、2 未生效（缺少权限）
int goState(void);
int goPaused(void);
int goBlockedToday(void);
int goDebounceMS(void);
void goTogglePause(void);
void goSetDebounce(int ms);
int goAutostartEnabled(void);
void goToggleAutostart(void);
void goOpenLog(void);
void goRevealConfig(void);
void goQuit(void);
void goCheckUpdate(void);

// CGEventTap 回调：返回 1 表示拦截这次按键
int goHandleKeyCode(uint32_t keyCode, int down, int autoRepeat);

#endif
