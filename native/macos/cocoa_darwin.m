#import <Cocoa/Cocoa.h>
#import <dispatch/dispatch.h>

#include "bridge.h"

// 菜单栏 UI：状态栏图标 + 下拉菜单。所有函数都由 Go 在主线程调用。

static NSStatusItem *gStatusItem = nil;
static NSMenuItem *gStatusMenuItem = nil;
static NSMenuItem *gPauseItem = nil;
static NSMenuItem *gAutoItem = nil;
static NSMenuItem *gCustomItem = nil;
static NSMenuItem *gDebounceMenuItem = nil;
static NSMenuItem *gPresetItems[4] = {nil, nil, nil, nil};
static const int kPresets[4] = {100, 80, 67, 50};
static NSImage *gNormalIcon = nil;
static NSImage *gPausedIcon = nil;
// 第二套样式「脉冲屏」：精简版（uiInit 传 NULL）不加载，菜单里也不显示切换项
static NSImage *gNormalAltIcon = nil;
static NSImage *gPausedAltIcon = nil;
static NSMenuItem *gStyleItems[2] = {nil, nil};
// 精简版（-tags noupdate）不构建检查更新，菜单里也不显示
static int gUpdateEnabled = 0;

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

- (void)checkUpdate:(id)sender {
    goCheckUpdate();
}

- (void)revealConfig:(id)sender {
    goRevealConfig();
}

- (void)setIconStyle:(id)sender {
    goSetIconStyle((int)[sender tag]);
    refreshUI();
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

void uiInit(const void *normalPNG, int normalLen, const void *pausedPNG, int pausedLen,
            const void *normalAltPNG, int normalAltLen, const void *pausedAltPNG, int pausedAltLen,
            const char *version, int updateEnabled) {
    @autoreleasepool {
        gUpdateEnabled = updateEnabled;
        [NSApplication sharedApplication];
        // 只显示菜单栏图标，不在 Dock 里出现
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];

        gNormalIcon = imageFromPNG(normalPNG, normalLen);
        gPausedIcon = imageFromPNG(pausedPNG, pausedLen);
        if (normalAltPNG != NULL && pausedAltPNG != NULL) {
            gNormalAltIcon = imageFromPNG(normalAltPNG, normalAltLen);
            gPausedAltIcon = imageFromPNG(pausedAltPNG, pausedAltLen);
        }
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

        // 防抖阈值放进子菜单，和「目标键盘」一致
        gDebounceMenuItem = [[NSMenuItem alloc] initWithTitle:@"防抖阈值" action:nil keyEquivalent:@""];
        NSMenu *debounceMenu = [[NSMenu alloc] init];
        [debounceMenu setTitle:@"防抖阈值"];
        [gDebounceMenuItem setSubmenu:debounceMenu];
        [menu addItem:gDebounceMenuItem];

        for (int i = 0; i < 4; i++) {
            NSMenuItem *item = addItem(debounceMenu,
                [NSString stringWithFormat:@"%dms", kPresets[i]],
                @selector(setDebouncePreset:));
            [item setTag:kPresets[i]];
            gPresetItems[i] = item;
        }

        gCustomItem = addItem(debounceMenu, @"自定义阈值…", @selector(customDebounce:));
        [menu addItem:[NSMenuItem separatorItem]];

        gPauseItem = addItem(menu, @"暂停防抖", @selector(togglePause:));
        gAutoItem = addItem(menu, @"开机自启", @selector(toggleAutostart:));
        if (gNormalAltIcon != nil) {
            NSMenuItem *styleItem = [[NSMenuItem alloc] initWithTitle:@"图标样式" action:nil keyEquivalent:@""];
            NSMenu *styleMenu = [[NSMenu alloc] init];
            [styleMenu setTitle:@"图标样式"];
            [styleItem setSubmenu:styleMenu];
            [menu addItem:styleItem];
            for (int i = 0; i < 2; i++) {
                NSMenuItem *item = addItem(styleMenu,
                    i == 0 ? @"键盘板" : @"脉冲屏",
                    @selector(setIconStyle:));
                [item setTag:i];
                gStyleItems[i] = item;
            }
        }
        [menu addItem:[NSMenuItem separatorItem]];

        addItem(menu, @"打开日志", @selector(openLog:));
        addItem(menu, @"配置文件位置", @selector(revealConfig:));
        if (gUpdateEnabled) {
            addItem(menu, @"检查更新…", @selector(checkUpdate:));
        }
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

// uiUpdateAlert 供 Go 的更新检查使用：检查在后台 goroutine 里进行，
// 这里切回主线程弹窗；url 非空时提供「打开下载页」按钮。
void uiUpdateAlert(const char *title, const char *message, const char *url) {
    NSString *titleText = [NSString stringWithUTF8String:title];
    NSString *messageText = [NSString stringWithUTF8String:message];
    NSString *urlText = (url != NULL) ? [NSString stringWithUTF8String:url] : nil;

    dispatch_async(dispatch_get_main_queue(), ^{
        @autoreleasepool {
            [NSApplication sharedApplication];
            [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
            NSAlert *alert = [[NSAlert alloc] init];
            [alert setMessageText:titleText];
            [alert setInformativeText:messageText];

            BOOL hasURL = (urlText != nil && [urlText length] > 0);
            if (hasURL) {
                [alert addButtonWithTitle:@"打开下载页"];
                [alert addButtonWithTitle:@"稍后"];
            } else {
                [alert addButtonWithTitle:@"好"];
            }

            [NSApp activateIgnoringOtherApps:YES];
            NSModalResponse resp = [alert runModal];
            if (hasURL && resp == NSAlertFirstButtonReturn) {
                [[NSWorkspace sharedWorkspace] openURL:[NSURL URLWithString:urlText]];
            }
        }
    });
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
    for (int i = 0; i < 4; i++) {
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

    // 子菜单标题带上当前阈值，收起时也能看到
    [gDebounceMenuItem setTitle:[NSString stringWithFormat:@"防抖阈值（%dms）", ms]];

    [gPauseItem setState:paused != 0 ? NSControlStateValueOn : NSControlStateValueOff];
    [gAutoItem setState:goAutostartEnabled() != 0 ? NSControlStateValueOn : NSControlStateValueOff];

    // 图标样式：标准版才有多套，样式号非 0 但缺备用图（精简版）时退回键盘板
    int style = goIconStyle();
    if (style != 0 && (gNormalAltIcon == nil || gPausedAltIcon == nil)) {
        style = 0;
    }
    NSImage *normal = (style == 1) ? gNormalAltIcon : gNormalIcon;
    NSImage *pausedImg = (style == 1) ? gPausedAltIcon : gPausedIcon;
    for (int i = 0; i < 2; i++) {
        if (gStyleItems[i] != nil) {
            [gStyleItems[i] setState:(style == i) ? NSControlStateValueOn : NSControlStateValueOff];
        }
    }

    NSImage *icon = (paused != 0) ? pausedImg : normal;
    if (icon != nil) {
        [[gStatusItem button] setImage:icon];
    }

    NSString *tip = [NSString stringWithFormat:@"键盘防抖 %dms · %@", ms, stateText];
    // NSStatusItem 的 setToolTip 自 10.14 起废弃，改用 button.toolTip（系统要求 10.15+）
    [[gStatusItem button] setToolTip:tip];
}
