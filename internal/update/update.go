// Package update 通过 GitHub Releases 检查是否有新版本。
//
// 只做「检查 + 提示」，不下载、不替换程序：两个平台的产物都未签名，
// 自动替换会触发 SmartScreen / Gatekeeper，macOS 替换后还可能丢失输入监控授权。
package update

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// latestURL 是 GitHub 的「最新发布」接口，仓库为公开仓库，无需鉴权。
const latestURL = "https://api.github.com/repos/singcl/keyboard/releases/latest"

// Release 是最新发布的信息。
type Release struct {
	Tag string // 例如 v1.2.4
	URL string // 发布页面地址
}

// Latest 查询 GitHub 上的最新发布版本。
func Latest() (Release, error) {
	client := &http.Client{Timeout: 8 * time.Second}
	req, err := http.NewRequest(http.MethodGet, latestURL, nil)
	if err != nil {
		return Release{}, err
	}
	req.Header.Set("User-Agent", "keyboard-plus")
	req.Header.Set("Accept", "application/vnd.github+json")

	resp, err := client.Do(req)
	if err != nil {
		return Release{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return Release{}, fmt.Errorf("GitHub 返回 %s", resp.Status)
	}

	var body struct {
		Tag     string `json:"tag_name"`
		HTMLURL string `json:"html_url"`
	}
	if err := json.NewDecoder(io.LimitReader(resp.Body, 1<<20)).Decode(&body); err != nil {
		return Release{}, err
	}
	if body.Tag == "" || body.HTMLURL == "" {
		return Release{}, errors.New("发布信息不完整")
	}
	return Release{Tag: body.Tag, URL: body.HTMLURL}, nil
}

// Newer 判断 latest 是否比 current 新；任一版本号无法解析时返回 false。
func Newer(current, latest string) bool {
	c, ok := parse(current)
	if !ok {
		return false
	}
	l, ok := parse(latest)
	if !ok {
		return false
	}
	for i := 0; i < len(c) || i < len(l); i++ {
		var a, b int
		if i < len(c) {
			a = c[i]
		}
		if i < len(l) {
			b = l[i]
		}
		if a != b {
			return b > a
		}
	}
	return false
}

// IsVersion 判断字符串是否为可比较的版本号，如 1.2.3 或 v1.2.3。
// 开发构建（dev-abc1234）等无法解析的版本号返回 false。
func IsVersion(v string) bool {
	_, ok := parse(v)
	return ok
}

// parse 把 1.2.3 / v1.2.3 解析成数字切片；带前后缀等无法解析时返回 false。
func parse(v string) ([]int, bool) {
	v = strings.TrimPrefix(strings.TrimSpace(v), "v")
	if v == "" {
		return nil, false
	}

	parts := strings.Split(v, ".")
	nums := make([]int, 0, len(parts))
	for _, p := range parts {
		n, err := strconv.Atoi(p)
		if err != nil || n < 0 {
			return nil, false
		}
		nums = append(nums, n)
	}
	return nums, true
}
