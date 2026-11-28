// macOS 实现：写 ~/Library/LaunchAgents/com.singcl.keyboard-plus.plist。

#[cfg(target_os = "macos")]

use std::io;
use std::path::PathBuf;

const LABEL: &str = "com.singcl.keyboard-plus";

const PLIST_TEMPLATE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{label}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{exe}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>ProcessType</key>
	<string>Interactive</string>
</dict>
</plist>
"#;

// plistPath：返回 LaunchAgents 路径并确保目录存在。
fn plist_path() -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "$HOME 未设置"))?;
    let dir = home.join("Library").join("LaunchAgents");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("{LABEL}.plist")))
}

/// 判断当前 exe 是否已写入开机自启配置。
pub fn enabled() -> bool {
    let Ok(path) = plist_path() else { return false };
    let Ok(text) = std::fs::read_to_string(path) else { return false };
    let Ok(exe) = std::env::current_exe() else { return true };
    text.contains(&xml_escape(&exe.to_string_lossy()))
}

/// 开启或关闭开机自启；写入的 LaunchAgent 在下次登录时生效。
pub fn set(enabled: bool) -> io::Result<()> {
    let path = plist_path()?;

    if !enabled {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    } else {
        let exe = std::env::current_exe()?;
        let content = PLIST_TEMPLATE
            .replace("{label}", LABEL)
            .replace("{exe}", &xml_escape(&exe.to_string_lossy()));
        std::fs::write(path, content)
    }
}

// 与 Go 版 html.EscapeString 等价的 XML 五字符转义（路径一般只涉及 & 和 <）。
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&#39;"),
            '"' => out.push_str("&#34;"),
            _ => out.push(c),
        }
    }
    out
}
