#include <ApplicationServices/ApplicationServices.h>
#include <stdbool.h>

#include "bridge.h"

static CFMachPortRef gTap = NULL;
static CFRunLoopSourceRef gSource = NULL;

// keyboardTapCallback 由系统在主线程的 run loop 里调用。
// 返回 NULL 表示丢弃这次事件（拦截），否则原样放行。
static CGEventRef keyboardTapCallback(CGEventTapProxy proxy, CGEventType type,
                                      CGEventRef event, void *context) {
    (void)proxy;
    (void)context;

    // 系统可能因为回调超时或用户输入而关闭监听，这里重新打开
    if (type == kCGEventTapDisabledByTimeout || type == kCGEventTapDisabledByUserInput) {
        if (gTap != NULL) {
            CGEventTapEnable(gTap, true);
        }
        return event;
    }

    if (type != kCGEventKeyDown && type != kCGEventKeyUp) {
        return event;
    }

    int64_t keyCode = CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
    int64_t repeat = CGEventGetIntegerValueField(event, kCGKeyboardEventAutorepeat);
    if (goHandleKeyCode((uint32_t)keyCode, type == kCGEventKeyDown, repeat != 0) != 0) {
        return NULL;
    }
    return event;
}

// kbdTapStart 创建并启用键盘事件监听，返回 0 表示成功。
int kbdTapStart(void) {
    if (gTap != NULL) {
        return 0;
    }

    CGEventMask mask = CGEventMaskBit(kCGEventKeyDown) | CGEventMaskBit(kCGEventKeyUp);
    gTap = CGEventTapCreate(kCGSessionEventTap, kCGHeadInsertEventTap,
                            kCGEventTapOptionDefault, mask,
                            keyboardTapCallback, NULL);
    if (gTap == NULL) {
        return 1;
    }

    gSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, gTap, 0);
    CFRunLoopAddSource(CFRunLoopGetMain(), gSource, kCFRunLoopCommonModes);
    CGEventTapEnable(gTap, true);
    return 0;
}

// kbdTapStop 停止监听并释放资源。
void kbdTapStop(void) {
    if (gTap == NULL) {
        return;
    }

    CGEventTapEnable(gTap, false);
    if (gSource != NULL) {
        CFRunLoopRemoveSource(CFRunLoopGetMain(), gSource, kCFRunLoopCommonModes);
        CFRelease(gSource);
        gSource = NULL;
    }
    CFRelease(gTap);
    gTap = NULL;
}

// kbdAccessTrusted 报告是否已有键盘监听权限：输入监控（10.15+）或辅助功能。
int kbdAccessTrusted(void) {
    if (CGPreflightListenEventAccess()) {
        return 1;
    }
    return AXIsProcessTrusted() ? 1 : 0;
}

// kbdAccessPrompt 触发系统授权引导，返回调用后是否已获得权限。
int kbdAccessPrompt(void) {
    if (kbdAccessTrusted()) {
        return 1;
    }

    // 输入监控权限：系统会弹窗并在「系统设置」中列出本程序
    CGRequestListenEventAccess();

    // 辅助功能权限的引导弹窗（旧系统或需要合成按键时也会用到）
    const void *keys[] = {kAXTrustedCheckOptionPrompt};
    const void *values[] = {kCFBooleanTrue};
    CFDictionaryRef options = CFDictionaryCreate(kCFAllocatorDefault, keys, values, 1,
                                                 &kCFTypeDictionaryKeyCallBacks,
                                                 &kCFTypeDictionaryValueCallBacks);
    if (options != NULL) {
        AXIsProcessTrustedWithOptions(options);
        CFRelease(options);
    }

    return kbdAccessTrusted();
}
