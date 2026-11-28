// 托盘图标：内嵌 PNG 解码成 32 位 HICON（正常绿色版 + 暂停珊瑚色版，
// 两枚图标都来自 assets 的 256x256 PNG 母版）。

use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

/// 正常与暂停两枚图标；程序退出时随进程一并释放（托盘常驻，无需显式销毁）。
pub(crate) struct Icons {
    pub normal: HICON,
    pub paused: HICON,
}

/// 从内嵌 PNG 解码并生成两枚 HICON；解码失败返回错误文本。
pub(crate) fn load(normal_png: &[u8], paused_png: &[u8]) -> Result<Icons, String> {
    let (w, h, rgba) = crate::assets::decode_rgba8(normal_png)?;
    let normal = icon_from_rgba(w, h, &rgba)?;
    let (w, h, rgba) = crate::assets::decode_rgba8(paused_png)?;
    let paused = icon_from_rgba(w, h, &rgba)?;
    Ok(Icons { normal, paused })
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetIconInfo;

    #[test]
    fn embedded_icon_loads() {
        // 回归：内嵌图标曾是调色板 PNG（colortype 3 + tRNS），decode_rgba8 必须兼容
        let icons = super::load(crate::assets::ICON_PNG, crate::assets::ICON_PAUSED_PNG)
            .expect("内嵌图标应能加载");
        assert!(!icons.normal.0.is_null());
        assert!(!icons.paused.0.is_null());
    }

    // 读出 HICON 的 32 位 BGRA 像素（自顶向下），验证两枚图标都与内嵌 PNG 逐像素一致。
    #[test]
    fn icon_pixels_match_embedded_png() {
        let (w, h, normal_rgba) = crate::assets::decode_rgba8(crate::assets::ICON_PNG).unwrap();
        let (_, _, paused_rgba) = crate::assets::decode_rgba8(crate::assets::ICON_PAUSED_PNG).unwrap();
        let icons = load(crate::assets::ICON_PNG, crate::assets::ICON_PAUSED_PNG).unwrap();

        let normal_px = icon_pixels(icons.normal, w, h);
        let paused_px = icon_pixels(icons.paused, w, h);
        assert_eq!(normal_px.len(), (w * h * 4) as usize);
        assert_eq!(paused_px.len(), (w * h * 4) as usize);

        // got=[B,G,R,A] vs px=[R,G,B,A]
        for (name, rgba, pxs) in [
            ("彩色", &normal_rgba, &normal_px),
            ("暂停", &paused_rgba, &paused_px),
        ] {
            for (i, px) in rgba.chunks_exact(4).enumerate() {
                let got = &pxs[i * 4..i * 4 + 4];
                assert_eq!(got[0], px[2], "{name}图标像素 {i} B 通道不符");
                assert_eq!(got[1], px[1], "{name}图标像素 {i} G 通道不符");
                assert_eq!(got[2], px[0], "{name}图标像素 {i} R 通道不符");
                assert_eq!(got[3], px[3], "{name}图标像素 {i} alpha 不符");
            }
        }
    }

    fn icon_pixels(icon: HICON, w: u32, h: u32) -> Vec<u8> {
        // SAFETY: 测试内创建/读取自建的 HICON 与位图，全部在本函数内释放。
        unsafe {
            let mut info = ICONINFO::default();
            GetIconInfo(icon, &mut info).expect("GetIconInfo 失败");

            let mut bm = BITMAP::default();
            let n = GetObjectW(
                HGDIOBJ(info.hbmColor.0),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bm as *mut _ as *mut _),
            );
            assert_eq!(n as usize, std::mem::size_of::<BITMAP>());
            assert_eq!((bm.bmWidth as u32, bm.bmHeight as u32), (w, h));
            assert_eq!(bm.bmBitsPixel, 32, "颜色位图应为 32bpp");

            let hdc = CreateCompatibleDC(None);
            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32), // 自顶向下，与创建时一致
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut buf = vec![0u8; (w * h * 4) as usize];
            let lines = GetDIBits(
                hdc,
                info.hbmColor,
                0,
                h,
                Some(buf.as_mut_ptr().cast()),
                &mut bi,
                DIB_RGB_COLORS,
            );
            let _ = DeleteDC(hdc);
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
            assert_eq!(lines, h as i32, "GetDIBits 读取行数不足");
            buf
        }
    }
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

        // 全零掩码 = 不遮蔽任何像素，透明度完全交给 alpha 通道。
        // 注意 CreateBitmap 的 lpBits 传 None 时位内容是「未初始化」的，
        // 垃圾掩码会把图标随机打洞（表现为黑块），必须显式写入全零。
        let mask_bits = vec![0u8; (((w as usize + 31) / 32) * 4) * h as usize];
        let mask = CreateBitmap(
            w as i32,
            h as i32,
            1,
            1,
            Some(mask_bits.as_ptr().cast()),
        );

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
