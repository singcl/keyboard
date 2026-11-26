#import <Cocoa/Cocoa.h>

#include "_cgo_export.h"

// 菜单栏 UI：状态栏图标 + 下拉菜单。所有函数都由 Go 在主线程调用。

static NSStatusItem *gStatusItem = nil;
static NSMenuItem *gStatusMenuItem = nil;
static NSMenuItem *gPauseItem = nil;
static NSMenuItem *gAutoItem = nil;
static NSMenuItem *gCustomItem = nil;
static NSMenuItem *gPresetItems[3] = {nil, nil, nil};
static const int kPresets[3] = {100, 67, 50};
static NSImage *gNormalIcon = nil;
static NSImage *gPausedIcon = nil;

static void refreshUI(void);

@interface KeyboardMenu : NSObject <NSMenuDelegate>
@end

@implementation KeyboardMenu

// 菜单即将显示时刷新勾选状态与统计数字
- (void)menuNeedsUpdate:(NSMenu *)menu {
    refreshUI();
}

- (void)togglePause:(id)sender {
    goTogglePause();
    refreshUI();
}

- (void)setDebouncePreset:(id)sender {
    goSetDebounce((int)[sender tag]);
    refreshUI();
}

- (void)customDebounce:(id)sender {
    NSAlert *alert = [[NSAlert alloc] init];
    [alert setMessageText:@"自定义防抖阈值"];
    [alert setInformativeText:@"同一颗键在多少毫秒内重新按下视为误触发（10-1000）："];
    [alert addButtonWithTitle:@"确定"];
    [alert addButtonWithTitle:@"取消"];

    NSTextField *input = [[NSTextField alloc] initWithFrame:NSMakeRect(0, 0, 200, 24)];
    [input setStringValue:[NSString stringWithFormat:@"%d", goDebounceMS()]];
    [alert setAccessoryView:input];
    [alert.window setInitialFirstResponder:input];

    [NSApp activateIgnoringOtherApps:YES];
    if ([alert runModal] == NSAlertFirstButtonReturn) {
        goSetDebounce([input intValue]);
    }
    refreshUI();
}

- (void)toggleAutostart:(id)sender {
    goToggleAutostart();
    refreshUI();
}

- (void)openLog:(id)sender {
    goOpenLog();
}

- (void)revealConfig:(id)sender {
    goRevealConfig();
}

- (void)quitApp:(id)sender {
    goQuit();
}

@end

static KeyboardMenu *gMenuTarget = nil;

static NSMenuItem *addItem(NSMenu *menu, NSString *title, SEL action) {
    NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:title action:action keyEquivalent:@""];
    [item setTarget:gMenuTarget];
    [menu addItem:item];
    return item;
}

static NSImage *imageFromPNG(const void *data, int length) {
    NSData *raw = [NSData dataWithBytes:data length:(NSUInteger)length];
    NSImage *image = [[NSImage alloc] initWithData:raw];
    [image setSize:NSMakeSize(18, 18)];
    return image;
}

void uiInit(const void *normalPNG, int normalLen, const void *grayPNG, int grayLen, const char *version) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        // 只显示菜单栏图标，不在 Dock 里出现
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];

        gNormalIcon = imageFromPNG(normalPNG, normalLen);
        gPausedIcon = imageFromPNG(grayPNG, grayLen);
        gMenuTarget = [[KeyboardMenu alloc] init];

        gStatusItem = [[NSStatusBar systemStatusBar] statusItemWithLength:NSSquareStatusItemLength];
        [[gStatusItem button] setImage:gNormalIcon];

        NSMenu *menu = [[NSMenu alloc] init];
        [menu setDelegate:gMenuTarget];

        // 菜单顶部状态行，直接显示当前是否在拦截
        gStatusMenuItem = [[NSMenuItem alloc] initWithTitle:@"状态" action:nil keyEquivalent:@""];
        [gStatusMenuItem setEnabled:NO];
        [menu addItem:gStatusMenuItem];
        [menu addItem:[NSMenuItem separatorItem]];

        for (int i = 0; i < 3; i++) {
            NSMenuItem *item = addItem(menu,
                [NSString stringWithFormat:@"防抖 %dms", kPresets[i]],
                @selector(setDebouncePreset:));
            [item setTag:kPresets[i]];
            gPresetItems[i] = item;
        }

        gCustomItem = addItem(menu, @"自定义阈值…", @selector(customDebounce:));
        [menu addItem:[NSMenuItem separatorItem]];

        gPauseItem = addItem(menu, @"暂停防抖", @selector(togglePause:));
        gAutoItem = addItem(menu, @"开机自启", @selector(toggleAutostart:));
        [menu addItem:[NSMenuItem separatorItem]];

        addItem(menu, @"打开日志", @selector(openLog:));
        addItem(menu, @"配置文件位置", @selector(revealConfig:));
        [menu addItem:[NSMenuItem separatorItem]];

        NSMenuItem *versionItem = [[NSMenuItem alloc]
            initWithTitle:[NSString stringWithUTF8String:version] action:nil keyEquivalent:@""];
        [versionItem setEnabled:NO];
        [menu addItem:versionItem];
        [menu addItem:[NSMenuItem separatorItem]];

        addItem(menu, @"退出", @selector(quitApp:));

        [gStatusItem setMenu:menu];
        refreshUI();
    }
}

void uiRun(void) {
    [NSApp run];
}

// uiRefresh 供 Go 在状态变化（如拦截计数增加）后刷新菜单栏显示。
void uiRefresh(void) {
    refreshUI();
}

void uiAlert(const char *title, const char *message) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
        NSAlert *alert = [[NSAlert alloc] init];
        [alert setMessageText:[NSString stringWithUTF8String:title]];
        [alert setInformativeText:[NSString stringWithUTF8String:message]];
        [alert addButtonWithTitle:@"好"];
        [NSApp activateIgnoringOtherApps:YES];
        [alert runModal];
    }
}

void uiPermissionAlert(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        NSAlert *alert = [[NSAlert alloc] init];
        [alert setMessageText:@"无法监听键盘事件"];
        [alert setInformativeText:@"请在「系统设置 → 隐私与安全性 → 输入监控」中勾选本程序，然后重新启动。"];
        [alert addButtonWithTitle:@"打开系统设置"];
        [alert addButtonWithTitle:@"稍后"];
        [NSApp activateIgnoringOtherApps:YES];

        if ([alert runModal] == NSAlertFirstButtonReturn) {
            NSURL *url = [NSURL URLWithString:
                @"x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"];
            [[NSWorkspace sharedWorkspace] openURL:url];
        }
    }
}

static void refreshUI(void) {
    if (gStatusItem == nil) {
        return;
    }

    int state = goState();
    int paused = goPaused();
    int ms = goDebounceMS();

    NSString *stateText = @"运行中";
    if (paused != 0) {
        stateText = @"已暂停";
    } else if (state == 2) {
        stateText = @"未生效（缺少权限）";
    }

    NSString *status = [NSString stringWithFormat:@"状态：%@", stateText];
    if (goBlockedToday() > 0) {
        status = [status stringByAppendingFormat:@" · 今日拦截 %d 次", goBlockedToday()];
    }
    [gStatusMenuItem setTitle:status];

    BOOL custom = YES;
    for (int i = 0; i < 3; i++) {
        BOOL on = (kPresets[i] == ms);
        [gPresetItems[i] setState:on ? NSControlStateValueOn : NSControlStateValueOff];
        if (on) {
            custom = NO;
        }
    }
    [gCustomItem setState:custom ? NSControlStateValueOn : NSControlStateValueOff];
    // 自定义阈值时直接显示当前值，不用打开输入框才能看到
    if (custom) {
        [gCustomItem setTitle:[NSString stringWithFormat:@"自定义阈值（%dms）…", ms]];
    } else {
        [gCustomItem setTitle:@"自定义阈值…"];
    }

    [gPauseItem setState:paused != 0 ? NSControlStateValueOn : NSControlStateValueOff];
    [gAutoItem setState:goAutostartEnabled() != 0 ? NSControlStateValueOn : NSControlStateValueOff];

    NSImage *icon = (paused != 0) ? gPausedIcon : gNormalIcon;
    if (icon != nil) {
        [[gStatusItem button] setImage:icon];
    }

    NSString *tip = [NSString stringWithFormat:@"键盘防抖 %dms · %@", ms, stateText];
    [gStatusItem setToolTip:tip];
    [[gStatusItem button] setToolTip:tip];
}
