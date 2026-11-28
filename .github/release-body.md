## 本次更新

- 「关于作者」支持一键打开两个链接：「是」打开作者个人主页（https://imcoco.top），「否」打开项目主页（GitHub），右上角 × 仅关闭
- 按钮文案精简，Windows 与 macOS 表现一致
- 精简版同样包含；功能与配置无变化，直接替换升级即可

## 下载

| 平台 | 标准版（含检查更新） | 精简版（不含联网检查） |
| --- | --- | --- |
| Windows x64 | `keyboard-plus-windows-amd64.zip` | `keyboard-plus-windows-amd64-slim.zip` |
| macOS（Apple Silicon / Intel） | `keyboard-plus-macos-universal.zip` | `keyboard-plus-macos-universal-slim.zip` |

两个版本功能一致：精简版只是去掉「检查更新」（不链接任何 HTTP/TLS 代码，体积约为标准版的四分之一，不访问网络）；配置、日志与开机自启共用。

## 使用

- **Windows**：解压后双击 `keyboard-plus.exe`，右键任务栏右下角的托盘图标可设置防抖阈值、暂停拦截、开机自启、目标键盘等。
- **macOS**：解压后运行 `keyboard-plus`，图标出现在菜单栏。首次启动需在「系统设置 → 隐私与安全性 → 输入监控」中勾选本程序，否则收不到按键；程序未签名，若提示「无法打开」，在「隐私与安全性」中点「仍要打开」。**注意：macOS 版本尚未经真机验证，可能存在未知问题，欢迎在 Issues 反馈。**

Go 实现保留在 [`go`](https://github.com/singcl/keyboard/tree/go) 分支，仍可自行构建 1.x。

详细说明与各平台功能差异见 [README](https://github.com/singcl/keyboard#readme)。

**完整改动**：https://github.com/singcl/keyboard/commits/v2.4.0
