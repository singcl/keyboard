# 防止某键盘误触发

新买的键盘用了几个月，一个问题越来越明显，就是误触发，按一次，重复响应多次，随时随地敲错特别烦。

就算我按得再轻也没用。

这个问题用 win 自带的 筛选键功能无法解决。

然而键盘超出了包退期，要自己花运费寄过去修QAQ！

所以我写了这个小程序，屏蔽重复触发。

希望能帮到不小心买到类似键盘的你。

解压后，双击里面的 keyboard.exe 运行。

托盘图标右键可以设置防抖阈值、暂停拦截、开机自启、识别键盘设备、选择目标键盘、查看/清空日志、定位配置文件。暂停时托盘图标会变灰，菜单第一行也会显示当前状态。开启「开机自启」后就不需要每次手动双击了。

下载地址：https://github.com/garfeng/keyboard/releases/latest

## 构建

需要 Go 1.22+，项目无 cgo 依赖，在 Windows 下执行：

```
build.bat      # Windows cmd / PowerShell
./build.sh     # Git Bash / WSL
```

或手动执行：

```
CGO_ENABLED=0 go build -ldflags "-w -s -H windowsgui" -o keyboard.exe ./cmd/keyboard
```

产物为当前目录下的 `keyboard.exe`，`-H windowsgui` 表示运行时不显示控制台窗口。

如果修改了 `internal/assets/icon.ico`、`internal/assets/keyboard.manifest` 或 `winres/winres.json`（版本号、文件描述等），需要重新生成资源后再编译：

```
go run github.com/tc-hib/go-winres@latest make --in winres/winres.json --out cmd/keyboard/rsrc.syso --arch amd64 --no-suffix
```

## 项目结构

```
.
├── cmd/keyboard/              # 程序入口：托盘图标、菜单
│   └── rsrc.syso              # 图标、清单与版本信息（go-winres 生成）
├── internal/
│   ├── assets/                # 图标与清单；icon.png 通过 go:embed 编入程序
│   ├── autostart/             # 开机自启（注册表 Run 键）
│   ├── config/                # 配置持久化（防抖阈值、目标键盘）
│   ├── devices/               # 键盘设备识别（Raw Input）
│   ├── hook/                  # Win32 低级键盘钩子与拦截逻辑
│   ├── logging/               # 拦截日志（异步写入、轮转、清空）
│   └── singleinstance/        # 单实例互斥量
├── winres/
│   └── winres.json            # 图标、清单与版本信息资源定义
├── .github/workflows/build.yml # CI：自动构建并打包发布
├── build.bat / build.sh       # 本地构建脚本
├── go.mod
└── README.md
```

## 拦截日志

被拦截的按键记录在 `%LOCALAPPDATA%\keyboard\keyboard.log`，托盘右键「查看日志」会在内置窗口里显示最后 500 行并每 2 秒自动刷新，「清空日志」可删除；「配置文件位置」会在资源管理器中定位 `config.json`。

注意：日志包含被拦截按键的名称和键码，属于输入信息，分享或反馈问题前请先清空或自行删改。

## 按键盘设备过滤

托盘菜单「识别键盘设备」会列出当前系统的键盘设备（品牌、VID/PID、设备路径），方便你确认是哪一把键盘有问题。部分键盘的 VID/PID 是公版或套用的，此时以设备名（如 `LANGTU BT5.0_3`）或设备路径为准。

「目标键盘」子菜单可以选定要关注的键盘（按设备名或设备路径匹配，重启后保留），每次右键打开菜单时会自动刷新设备列表；勾选「仅目标键盘在线时防抖」后，只有该键盘连接时才会启用拦截，拔掉或未连接时自动暂停，其他键盘单独使用时不受影响。

当前是否在拦截可以直接看托盘菜单第一行的状态（运行中 / 已暂停 / 已暂停（目标键盘未连接）），悬停图标也会显示；目标键盘上下线、自动暂停/恢复都会写入日志，便于事后核对。

但拦截本身仍是**对所有键盘生效**的：Windows 的低级键盘钩子（`WH_KEYBOARD_LL`）回调里没有设备标识，纯用户态程序无法判断按键来自哪把键盘。若确实需要只对指定设备生效，只能安装内核过滤驱动（如 Interception），代价是需要管理员权限、驱动签名，并会失去免安装单文件的分发方式。

## 更新记录

* 2026年11月：暂停时图标变灰，新增内置日志窗口与「配置文件位置」，加入 GitHub Actions 自动构建与发布

* 2026年11月：新增目标键盘选择与「仅目标键盘在线时防抖」，修复自动暂停不生效的问题

* 2026年11月：新增设备识别、配置持久化、开机自启、暂停防抖、日志清理

* 2026年11月：纯 Go 重写，新增拦截日志与内嵌图标

* 2022年9月28日：加系统托盘图标

* 2018年6月：基本功能完成