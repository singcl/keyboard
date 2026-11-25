// Package assets 提供编译期嵌入的静态资源。
package assets

import _ "embed"

// IconPNG 是托盘图标使用的 PNG 母版。
//
//go:embed icon.png
var IconPNG []byte
