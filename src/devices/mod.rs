// Package devices 枚举系统键盘设备，用于展示品牌/型号信息。
//
// 注意：低级键盘钩子拿不到设备信息，因此这里只能识别设备，不能按设备拦截。
// 枚举能力由各平台实现提供：Windows 见 devices::windows（Raw Input），
// 其他平台目前为占位实现；设备分组、文本整理等逻辑是平台无关的。
//
// macOS 没有设备识别与目标键盘功能，本模块绝大多数条目仅被 Windows 界面
// 使用；实现按平台中立保留（与 Go 版一致），整个模块豁免 dead_code。
#![allow(dead_code)]

use std::fmt::Write as _;
use std::sync::LazyLock;

/// 当前平台尚未实现设备枚举。
// 非 Windows 平台的占位实现与测试使用；Windows 构建下视为保留导出
#[allow(dead_code)]
pub const ERR_UNSUPPORTED: &str = "devices: 当前平台暂不支持设备枚举";

// 这些名称过于笼统，不能代表具体型号
const GENERIC_NAMES: &[&str] = &[
    "hid-compliant",
    "符合 hid",
    "hid keyboard",
    "hid 键盘",
    "bluetooth le",
    "蓝牙 le",
    "usb 输入设备",
    "usb input device",
    "usb composite device",
    "usb 复合设备",
    "pci to isa",
    "pci 到 isa 桥",
];

/// 一个键盘设备。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Device {
    /// 设备友好名或 HID 产品名，可能为空
    pub name: String,
    /// 根据 VID 推断的品牌，可能为空
    pub brand: String,
    /// USB 厂商 ID，如 046D
    pub vid: String,
    /// USB 产品 ID
    pub pid: String,
    /// 设备接口路径
    pub path: String,
}

impl Device {
    /// 用于展示的名称：友好名 > 品牌 > 路径推断。
    pub fn display_name_owned(&self) -> String {
        if !self.name.is_empty() {
            return self.name.trim().to_string();
        }
        if !self.brand.is_empty() {
            return format!("{} 键盘", self.brand);
        }
        short_name(&self.path).to_string()
    }

    /// 判断设备是否与指定目标匹配。
    /// 配置了路径时按路径精确匹配：同型号的多把键盘名称相同，只有路径能区分；
    /// 没有路径时才退回到按名称匹配。
    pub fn matches(&self, name: &str, path: &str) -> bool {
        if !path.is_empty() {
            return self.path == path;
        }
        !name.is_empty() && self.name == name
    }
}

/// 判断目标键盘当前是否在线；name 与 path 任一为空则不参与匹配。
pub fn online(name: &str, path: &str) -> bool {
    let Ok(list) = list() else {
        return false;
    };
    list.iter().any(|d| d.matches(name, path))
}

/// 去重后按实体键盘与系统/虚拟设备分组，组内按展示名排序。
pub fn group(list: Vec<Device>) -> (Vec<Device>, Vec<Device>) {
    let mut physical = Vec::with_capacity(list.len());
    let mut system = Vec::with_capacity(list.len());
    let mut seen = std::collections::HashSet::new();
    for d in list {
        // 同一把键盘可能有多个采集接口，按展示名+VID/PID 去重
        let key = format!("{}|{}|{}", d.display_name_owned(), d.vid, d.pid);
        if !seen.insert(key) {
            continue;
        }

        if !d.vid.is_empty() && !d.pid.is_empty() {
            physical.push(d);
        } else {
            system.push(d);
        }
    }

    let by_name = |items: &mut Vec<Device>| {
        items.sort_by_key(|d| d.display_name_owned().to_lowercase());
    };
    by_name(&mut physical);
    by_name(&mut system);
    (physical, system)
}

/// 返回一组设备各自用于展示的名称。
/// 名称重复的设备追加 VID/PID，避免菜单里出现多条一模一样的条目。
pub fn labels(list: &[Device]) -> Vec<String> {
    let mut counts = std::collections::HashMap::new();
    for d in list {
        *counts.entry(d.display_name_owned()).or_insert(0usize) += 1;
    }

    list.iter()
        .map(|d| {
            let label = d.display_name_owned();
            let count = counts.get(&label).copied().unwrap_or(0);
            if count > 1 && (!d.vid.is_empty() || !d.pid.is_empty()) {
                format!("{}（VID_{} / PID_{}）", label, d.vid, d.pid)
            } else {
                label
            }
        })
        .collect()
}

/// 把设备列表整理成便于阅读的分组文本：实体键盘在前，系统/虚拟设备在后。
pub fn format_list(list: &[Device], target_name: &str, target_path: &str) -> String {
    let (physical, system) = group(list.to_vec());

    let mut b = String::new();
    let _ = write!(
        b,
        "检测到键盘设备 {} 个（实体 {}，系统/虚拟 {}）。",
        physical.len() + system.len(),
        physical.len(),
        system.len()
    );

    let mut write_group = |title: &str, items: &[Device]| {
        if items.is_empty() {
            return;
        }
        let _ = write!(b, "\n\n{title}\n");
        let labels = labels(items);
        for (i, d) in items.iter().enumerate() {
            let _ = write!(b, "  {}. {}", i + 1, labels[i].trim());
            if d.matches(target_name, target_path) {
                b.push_str("（当前目标）");
            }
            if !d.vid.is_empty() || !d.pid.is_empty() {
                let _ = write!(b, "\n     VID_{} / PID_{}", d.vid, d.pid);
            }
            if i != items.len() - 1 {
                b.push('\n');
            }
        }
    };

    write_group("实体键盘", &physical);
    write_group("系统 / 虚拟设备（通常不是外接键盘）", &system);

    b.push_str("\n\n提示：拦截对所有键盘生效，指定目标只影响是否自动暂停。");
    b
}

pub(crate) fn is_generic_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    GENERIC_NAMES.iter().any(|g| lower.contains(g))
}

/// 从设备路径中解析出 VID/PID，兼容 VID_046D 与 BLE 的 VID&02046D 形式。
pub fn parse_ids(path: &str) -> (String, String) {
    let upper = path.to_uppercase();
    (parse_id(&upper, "VID"), parse_id(&upper, "PID"))
}

fn parse_id(s: &str, kind: &str) -> String {
    let needle = format!("{kind}_");
    if let Some(i) = s.find(&needle) {
        return first_hex4(&s[i + needle.len()..]);
    }

    let needle = format!("{kind}&");
    let Some(i) = s.find(&needle) else {
        return String::new();
    };
    let rest = &s[i + needle.len()..];
    // 蓝牙 LE 设备的路径形如 VID&02046D，前两位是固定前缀
    if rest.len() >= 6 && &rest[0..2] == "02" {
        let v = first_hex4(&rest[2..]);
        if !v.is_empty() {
            return v;
        }
    }
    first_hex4(rest)
}

fn first_hex4(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() < 4 {
        return String::new();
    }
    if !bytes[..4].iter().all(|c| c.is_ascii_digit() || c.is_ascii_uppercase() && *c <= b'F') {
        return String::new();
    }
    s[..4].to_string()
}

/// 从设备路径中提取一个可读的简短名称。
pub fn short_name(path: &str) -> String {
    if let Some(i) = path.find("HID#") {
        let mut rest = &path[i + "HID#".len()..];
        if let Some(j) = rest.find('#') {
            rest = &rest[..j];
        }
        if rest.starts_with('{') {
            return "蓝牙 HID 设备".to_string();
        }
        if let Some(j) = rest.find('&') {
            rest = &rest[..j];
        }
        return rest.to_string();
    }
    if let Some(i) = path.find("ACPI#") {
        let mut rest = &path[i + "ACPI#".len()..];
        if let Some(j) = rest.find('#') {
            rest = &rest[..j];
        }
        return format!("ACPI {rest}");
    }
    "键盘设备".to_string()
}

#[cfg(windows)]
mod windows;
#[cfg(not(windows))]
mod stub;

mod imp;

pub use imp::list;

// 根据 VID 推断品牌（Windows 实现也用它，与 Go 版同表）。
pub(crate) static VENDORS: LazyLock<std::collections::HashMap<&'static str, &'static str>> =
    LazyLock::new(|| {
        [
            ("03F0", "HP"),
            ("045E", "Microsoft"),
            ("046A", "CHERRY"),
            ("046D", "Logitech"),
            ("04D9", "Holtek"),
            ("05AC", "Apple"),
            ("0853", "Topre"),
            ("0B05", "ASUS"),
            ("1532", "Razer"),
            ("17EF", "Lenovo"),
            ("1B1C", "Corsair"),
            ("258A", "SinoWealth"),
            ("3434", "Keychron"),
            ("413C", "Dell"),
        ]
        .into_iter()
        .collect()
    });

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ids_cases() {
        let cases = [
            (r"\\?\HID#VID_046D&PID_C31C&MI_00#8&2f6a1c3&0&0000#{884b96c3-56ef-11d1-bc8c-00a0c91405dd}", "046D", "C31C"),
            (r"\\?\HID#VID_3434&PID_0280#7&1a2b3c4d&0&0000#{...}", "3434", "0280"),
            (r"\\?\HID#{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&02046d_PID&b34c_REV&0014_b02a6f00d815&Col01#9&128bb436&0&0000#{884b96c3-56ef-11d1-bc8c-00a0c91405dd}", "046D", "B34C"),
            (r"ACPI\PNP0303\4&2f6a1c3&0", "", ""),
        ];
        for (path, vid, pid) in cases {
            let (got_vid, got_pid) = parse_ids(path);
            assert_eq!((got_vid.as_str(), got_pid.as_str()), (vid, pid), "path: {path}");
        }
    }

    #[test]
    fn matches_by_path_then_name() {
        let d = Device {
            name: "LANGTU BT5.0_3".into(),
            path: r"\\?\HID#VID_046D&PID_B34C".into(),
            ..Default::default()
        };
        assert!(d.matches("LANGTU BT5.0_3", ""), "未配置路径时按名称应匹配");
        assert!(d.matches("", r"\\?\HID#VID_046D&PID_B34C"), "按路径应匹配");
        assert!(!d.matches("其他键盘", ""), "不同名称不应匹配");
        // 配置了路径就只按路径匹配，避免同型号的另一把键盘被误判为目标
        assert!(
            !d.matches("LANGTU BT5.0_3", r"\\?\HID#VID_046D&PID_B34C&MI_01"),
            "路径不同时不应再按同名匹配"
        );
    }

    #[test]
    fn labels_dedup_repeated_names() {
        let list = vec![
            Device { name: "LANGTU BT5.0_3".into(), vid: "046D".into(), pid: "B34C".into(), ..Default::default() },
            Device { name: "LANGTU BT5.0_3".into(), vid: "258A".into(), pid: "0049".into(), ..Default::default() },
            Device { name: "AULA-F99 5.0".into(), vid: "3554".into(), pid: "FA07".into(), ..Default::default() },
        ];
        let labels = labels(&list);
        assert!(labels[0].contains("046D") && labels[1].contains("258A"), "同名设备应追加 VID/PID 区分：{labels:?}");
        assert_eq!(labels[2], "AULA-F99 5.0", "不重名时不应追加信息");
    }

    #[test]
    fn format_list_groups_and_marks_target() {
        let list = vec![
            Device { name: "AULA-F99 5.0".into(), vid: "3554".into(), pid: "FA07".into(), ..Default::default() },
            Device { name: "AULA-F99 5.0".into(), vid: "3554".into(), pid: "FA07".into(), ..Default::default() },
            Device { name: "LANGTU BT5.0_3".into(), vid: "046D".into(), pid: "B34C".into(), ..Default::default() },
            Device { name: "ACPI HPQ8002".into(), ..Default::default() },
        ];

        let got = format_list(&list, "LANGTU BT5.0_3", "");
        assert_eq!(got.matches("AULA-F99 5.0").count(), 1, "重复设备应去重");
        assert!(got.contains("实体 2"), "实体设备统计错误：\n{got}");
        assert!(got.contains("LANGTU BT5.0_3（当前目标）"), "目标标记缺失：\n{got}");
        assert!(got.contains("系统 / 虚拟设备"), "缺少系统设备分组：\n{got}");
    }

    #[cfg(windows)]
    #[test]
    fn list_real_devices() {
        // 真机冒烟：只要求不报错并打印设备
        let list = list().expect("设备枚举失败");
        for d in &list {
            println!("{:?} brand={:?} vid={} pid={} path={}", d.display_name_owned(), d.brand, d.vid, d.pid, d.path);
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn list_unsupported_elsewhere() {
        assert!(matches!(list(), Err(ref e) if e.contains(ERR_UNSUPPORTED) || e.contains("不支持")));
    }
}
