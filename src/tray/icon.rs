// 托盘图标：内嵌 PNG 解码成 32 位 HICON（彩色 + 暂停时的灰度版）。
//
// 灰度换算与 Go 版 image.Gray 一致：L = (19595R + 38470G + 7471B + 1<<15) >> 24。

use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

/// 彩色与灰度两枚图标；程序退出时随进程一并释放（托盘常驻，无需显式销毁）。
pub(crate) struct Icons {
    pub normal: HICON,
    pub paused: HICON,
}

/// 从内嵌 PNG 解码并生成两枚 HICON；解码失败返回错误文本。
pub(crate) fn load(png_bytes: &[u8]) -> Result<Icons, String> {
    let (w, h, rgba) = crate::assets::decode_rgba8(png_bytes)?;
    let gray = to_gray(&rgba);
    let normal = icon_from_rgba(w, h, &rgba)?;
    let paused = icon_from_rgba(w, h, &gray)?;
    Ok(Icons { normal, paused })
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_icon_loads() {
        // 回归：内嵌图标是调色板 PNG（colortype 3 + tRNS），必须能解码成 HICON
        let icons = super::load(crate::assets::ICON_PNG).expect("内嵌图标应能加载");
        assert!(!icons.normal.0.is_null());
        assert!(!icons.paused.0.is_null());
    }
}

fn to_gray(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|px| {
            let (r, g, b, a) = (px[0] as u32, px[1] as u32, px[2] as u32, px[3]);
            let l = ((19595 * r + 38470 * g + 7471 * b + (1 << 15)) >> 24) as u8;
            [l, l, l, a]
        })
        .collect()
}

// 32 位 BGRA DIB（自顶向下）+ 全零单色掩码：透明度由每像素 alpha 决定。
fn icon_from_rgba(w: u32, h: u32, rgba: &[u8]) -> Result<HICON, String> {
    // SAFETY: 结构体字段手工填写；像素缓冲区指针来自 CreateDIBSection 的返回值。
    unsafe {
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // 负高度 = 自顶向下，PNG 行序一致
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(None, &bi, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|e| format!("创建图标位图失败：{e}"))?;
        if bits.is_null() {
            return Err("创建图标位图失败：无像素缓冲".into());
        }

        // RGBA -> BGRA（小端机器上 DIB 内存字节序）
        let dst = std::slice::from_raw_parts_mut(bits.cast::<u8>(), rgba.len());
        for (d, px) in dst.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
            d[0] = px[2];
            d[1] = px[1];
            d[2] = px[0];
            d[3] = px[3];
        }

        // 全零掩码 = 不遮蔽任何像素，透明度完全交给 alpha 通道
        let mask = CreateBitmap(w as i32, h as i32, 1, 1, None);

        let info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info).map_err(|e| format!("创建图标失败：{e}"))?;
        let _ = DeleteObject(HGDIOBJ(color.0));
        let _ = DeleteObject(HGDIOBJ(mask.0));
        Ok(icon)
    }
}
