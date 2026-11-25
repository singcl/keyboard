# 防止某键盘误触发

新买的键盘用了几个月，一个问题越来越明显，就是误触发，按一次，重复响应多次，随时随地敲错特别烦。

就算我按得再轻也没用。

这个问题用 win 自带的 筛选键功能无法解决。

然而键盘超出了包退期，要自己花运费寄过去修QAQ！

所以我写了这个小程序，屏蔽重复触发。

希望能帮到不小心买到类似键盘的你。

解压后，双击里面的 keyboard.exe 运行。

可以设置为开机启动，这样以后就不需要手动双击了。

下载地址：https://github.com/garfeng/keyboard/releases/latest

## 构建

需要 Go 1.18+，项目无 cgo 依赖，在 Windows 下执行：

```
CGO_ENABLED=0 go build -ldflags "-w -s -H windowsgui" -o keyboard.exe ./cmd/keyboard
```

产物为当前目录下的 `keyboard.exe`，`-H windowsgui` 表示运行时不显示控制台窗口。

如果修改了 `internal/assets/icon.ico` 或 `internal/assets/keyboard.manifest`，需要重新生成资源文件后再编译：

```
go run github.com/akavel/rsrc@latest -manifest internal/assets/keyboard.manifest -ico internal/assets/icon.ico -o cmd/keyboard/rsrc.syso -arch amd64
```

## 项目结构

```
.
├── cmd/keyboard/       # 程序入口：托盘图标、菜单
│   └── rsrc.syso       # 编译进 exe 的图标与清单（由 rsrc 生成）
├── internal/
│   ├── assets/         # 图标与清单；icon.png 通过 go:embed 编入程序
│   ├── hook/           # Win32 低级键盘钩子与拦截逻辑
│   └── logging/        # 拦截日志读写
├── go.mod
└── README.md
```

## 拦截日志

被拦截的按键记录在 `%LOCALAPPDATA%\keyboard\keyboard.log`，也可通过托盘右键菜单「打开日志」查看。

## 更新记录

* 2026年11月：纯 Go 重写，新增拦截日志与内嵌图标

* 2022年9月28日：加系统托盘图标

* 2018年6月：基本功能完成