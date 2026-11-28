// Windows 平台实现：Raw Input 枚举键盘设备，CM_* 读设备属性，HID 读产品名。

#[cfg(windows)]

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_Device_Interface_PropertyW, CM_Get_DevNode_PropertyW, CM_Get_Parent,
    CM_Locate_DevNodeW, CONFIGRET, CR_BUFFER_SMALL, CM_LOCATE_DEVNODE_NORMAL,
};
use windows::Win32::Devices::HumanInterfaceDevice::HidD_GetProductString;
use windows::Win32::Devices::Properties::{
    DEVPKEY_Device_BusReportedDeviceDesc, DEVPKEY_Device_FriendlyName, DEVPKEY_Device_InstanceId,
    DEVPROP_TYPE_STRING,
};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows::Win32::UI::Input::{
    GetRawInputDeviceList, GetRawInputDeviceInfoW, RAWINPUTDEVICELIST, RIM_TYPEKEYBOARD,
    RIDI_DEVICENAME,
};

use super::{is_generic_name, parse_ids, Device, VENDORS};

const MAX_PARENT_DEPTH: usize = 5;
const CONFIGRET_OK: CONFIGRET = CONFIGRET(0);

/// 枚举当前系统中的键盘设备。
pub fn list() -> Result<Vec<Device>, String> {
    let devices = unsafe {
        // 第一次调用取数量
        let mut count = 0u32;
        let size = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
        if GetRawInputDeviceList(None, &mut count, size) == u32::MAX {
            let e = windows::core::Error::from_thread();
            return Err(format!("devices: 枚举键盘设备失败：{e}"));
        }
        if count == 0 {
            return Ok(Vec::new());
        }

        let mut raw = vec![RAWINPUTDEVICELIST::default(); count as usize];
        if GetRawInputDeviceList(Some(raw.as_mut_ptr()), &mut count, size) == u32::MAX {
            let e = windows::core::Error::from_thread();
            return Err(format!("devices: 枚举键盘设备失败：{e}"));
        }
        raw
    };

    let mut result = Vec::with_capacity(devices.len());
    for item in devices {
        if item.dwType != RIM_TYPEKEYBOARD {
            continue;
        }
        let Ok(path) = device_name(item.hDevice) else { continue };
        if path.is_empty() {
            continue;
        }

        let mut name = friendly_name(&path);
        if name.is_empty() || is_generic_name(&name) {
            let product = product_name(&path);
            if !product.is_empty() && !is_generic_name(&product) {
                name = product;
            } else {
                name.clear();
            }
        }

        let (vid, pid) = parse_ids(&path);
        result.push(Device {
            name,
            brand: VENDORS.get(vid.as_str()).map(|s| s.to_string()).unwrap_or_default(),
            vid,
            pid,
            path,
        });
    }
    Ok(result)
}

fn device_name(handle: HANDLE) -> Result<String, String> {
    unsafe {
        let mut size = 0u32;
        let _ = GetRawInputDeviceInfoW(Some(handle), RIDI_DEVICENAME, None, &mut size);
        if size == 0 {
            return Err("devices: 取设备路径长度失败".into());
        }

        let mut buf = vec![0u16; size as usize];
        if GetRawInputDeviceInfoW(
            Some(handle),
            RIDI_DEVICENAME,
            Some(buf.as_mut_ptr().cast()),
            &mut size,
        ) == u32::MAX
        {
            return Err("devices: 取设备路径失败".into());
        }
        Ok(wide_to_string(&buf))
    }
}

// 依次从接口、设备节点及其父节点读取名称，优先返回具体型号名。
fn friendly_name(path: &str) -> String {
    let mut candidates: Vec<String> = Vec::new();

    if let Some(n) = interface_string_prop(path, &DEVPKEY_Device_FriendlyName) {
        if !n.is_empty() {
            candidates.push(n);
        }
    }

    if let Some(instance_id) = interface_string_prop(path, &DEVPKEY_Device_InstanceId) {
        if let Some(dev_inst) = locate_dev_node(&instance_id) {
            let mut cur = dev_inst;
            for _ in 0..MAX_PARENT_DEPTH {
                for key in [&DEVPKEY_Device_BusReportedDeviceDesc, &DEVPKEY_Device_FriendlyName] {
                    if let Some(n) = dev_node_string_prop(cur, key) {
                        if !n.is_empty() {
                            candidates.push(n);
                        }
                    }
                }
                match parent_dev_node(cur) {
                    Some(parent) => cur = parent,
                    None => break,
                }
            }
        }
    }

    for n in &candidates {
        if !is_generic_name(n) {
            return n.clone();
        }
    }
    candidates.first().cloned().unwrap_or_default()
}

fn locate_dev_node(instance_id: &str) -> Option<u32> {
    if instance_id.is_empty() {
        return None;
    }
    let wide = wide_from_str(instance_id);
    let mut dev_inst = 0u32;
    // SAFETY: 输出指针与宽字符串均在调用期间有效。
    let r = unsafe {
        CM_Locate_DevNodeW(&mut dev_inst, PCWSTR(wide.as_ptr()), CM_LOCATE_DEVNODE_NORMAL)
    };
    (r == CONFIGRET_OK).then_some(dev_inst)
}

fn parent_dev_node(dev_inst: u32) -> Option<u32> {
    let mut parent = 0u32;
    // SAFETY: 输出指针有效；ulflags 恒为 0。
    let r = unsafe { CM_Get_Parent(&mut parent, dev_inst, 0) };
    (r == CONFIGRET_OK).then_some(parent)
}

fn interface_string_prop(path: &str, key: &windows::Win32::Foundation::DEVPROPKEY) -> Option<String> {
    let wide = wide_from_str(path);
    // SAFETY: 属性键与缓冲区指针在调用期间有效；两段式调用（先长度后内容）与 Go 版一致。
    unsafe {
        read_string_prop(|buf, size, prop_type| {
            CM_Get_Device_Interface_PropertyW(
                PCWSTR(wide.as_ptr()),
                key,
                prop_type,
                buf,
                size,
                0,
            )
        })
    }
}

fn dev_node_string_prop(dev_inst: u32, key: &windows::Win32::Foundation::DEVPROPKEY) -> Option<String> {
    // SAFETY: 属性键与缓冲区指针在调用期间有效。
    unsafe {
        read_string_prop(|buf, size, prop_type| {
            CM_Get_DevNode_PropertyW(dev_inst, key, prop_type, buf, size, 0)
        })
    }
}

// 调用一次查询取长度，再取内容；返回 None 表示属性不存在或类型不对。
unsafe fn read_string_prop(
    query: impl Fn(Option<*mut u8>, *mut u32, *mut windows::Win32::Devices::Properties::DEVPROPTYPE) -> CONFIGRET,
) -> Option<String> {
    let mut prop_type = windows::Win32::Devices::Properties::DEVPROPTYPE(0);
    let mut size = 0u32;
    let r = query(None, &mut size, &mut prop_type);
    if r != CONFIGRET_OK && r != CR_BUFFER_SMALL {
        return None;
    }
    if size < 2 {
        return None;
    }

    let mut buf = vec![0u8; size as usize];
    let r = query(Some(buf.as_mut_ptr()), &mut size, &mut prop_type);
    if r != CONFIGRET_OK || prop_type != DEVPROP_TYPE_STRING {
        return None;
    }
    let wide: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(wide_to_string(&wide))
}

// 通过 HID 接口读取产品名，失败时返回空字符串。
fn product_name(path: &str) -> String {
    let wide = wide_from_str(path);
    // SAFETY: CreateFileW 只读方式打开 HID 接口；缓冲区为 128 个 u16。
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            0,
            FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0),
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    };
    let Ok(handle) = handle else {
        return String::new();
    };

    let mut buf = [0u16; 128];
    // SAFETY: 缓冲区在调用期间有效，长度按字节数传入。
    let ok = unsafe {
        HidD_GetProductString(
            handle,
            buf.as_mut_ptr().cast(),
            (buf.len() * 2) as u32,
        )
    };
    // SAFETY: 句柄由 CreateFileW 返回。
    unsafe {
        let _ = windows::Win32::Foundation::CloseHandle(handle);
    }
    if !ok {
        return String::new();
    }
    wide_to_string(&buf)
}

fn wide_from_str(s: &str) -> Vec<u16> {
    let wide: Vec<u16> = Path::new(s)
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    wide
}

fn wide_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}
