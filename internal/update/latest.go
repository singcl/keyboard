//go:build !noupdate

package update

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"time"
)

// Enabled 表示当前构建是否包含在线检查更新功能。
const Enabled = true

// latestURL 是 GitHub 的「最新发布」接口，仓库为公开仓库，无需鉴权。
const latestURL = "https://api.github.com/repos/singcl/keyboard/releases/latest"

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
