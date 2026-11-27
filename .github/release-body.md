## 本次更新

- 修复：跨天后托盘状态行的「今日拦截」计数未归零，一直显示前一天的数字

## 下载

| 平台 | 标准版（含检查更新） | 精简版（不含联网检查） |
| --- | --- | --- |
| Windows x64 | `keyboard-plus-windows-amd64.zip` | `keyboard-plus-windows-amd64-slim.zip` |
| macOS（Apple Silicon / Intel） | `keyboard-plus-macos-universal.zip` | `keyboard-plus-macos-universal-slim.zip` |

两个版本功能一致：精简版只是去掉「检查更新」（不链接 `net/http`，体积约一半，不访问网络）；配置、日志与开机自启共用。

## 使用

- **Windows**：解压后双击 `keyboard-plus.exe`，右键任务栏右下角的托盘图标可设置防抖阈值、暂停拦截、开机自启、目标键盘等。
- **macOS**：解压后运行 `keyboard-plus`，图标出现在菜单栏。首次启动需在「系统设置 → 隐私与安全性 → 输入监控」中勾选本程序，否则收不到按键；程序未签名，若提示「无法打开」，在「隐私与安全性」中点「仍要打开」。**注意：macOS 版本尚未经真机验证，可能存在未知问题，欢迎在 Issues 反馈。**

详细说明与各平台功能差异见 [README](https://github.com/singcl/keyboard#readme)。

**完整改动**：https://github.com/singcl/keyboard/commits/master
