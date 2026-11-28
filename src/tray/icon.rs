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
    use super::*;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetIconInfo;

    #[test]
    fn embedded_icon_loads() {
        // 回归：内嵌图标是调色板 PNG（colortype 3 + tRNS），必须能解码成 HICON
        let icons = super::load(crate::assets::ICON_PNG).expect("内嵌图标应能加载");
        assert!(!icons.normal.0.is_null());
        assert!(!icons.paused.0.is_null());
    }

    // 读出 HICON 的 32 位 BGRA 像素（自顶向下），验证灰色图标没有变成黑块。
    #[test]
    fn paused_icon_pixels_are_gray_with_alpha() {
        let (w, h, rgba) = crate::assets::decode_rgba8(crate::assets::ICON_PNG).unwrap();
        let gray = to_gray(&rgba);
        let icons = load(crate::assets::ICON_PNG).unwrap();

        let normal_px = icon_pixels(icons.normal, w, h);
        let paused_px = icon_pixels(icons.paused, w, h);
        assert_eq!(normal_px.len(), (w * h * 4) as usize);
        assert_eq!(paused_px.len(), (w * h * 4) as usize);

        // 彩色图标：与解码出的 RGBA 逐像素一致（got=[B,G,R,A] vs px=[R,G,B,A]）
        for (i, px) in rgba.chunks_exact(4).enumerate() {
            let got = &normal_px[i * 4..i * 4 + 4];
            assert_eq!(got[0], px[2], "彩色图标像素 {i} B 通道不符");
            assert_eq!(got[1], px[1], "彩色图标像素 {i} G 通道不符");
            assert_eq!(got[2], px[0], "彩色图标像素 {i} R 通道不符");
            assert_eq!(got[3], px[3], "彩色图标像素 {i} alpha 不符");
        }

        // 灰色图标（暂停态）：必须与灰度转换的期望值逐像素一致（保持 alpha，RGB 灰度）
        let (mut min_l, mut max_l, opaque) = (255u8, 0u8, std::cell::Cell::new(0usize));
        for (i, px) in gray.chunks_exact(4).enumerate() {
            let got = &paused_px[i * 4..i * 4 + 4];
            assert_eq!(got[0], px[0], "灰色图标像素 {i} B 不符");
            assert_eq!(got[1], px[1], "灰色图标像素 {i} G 不符");
            assert_eq!(got[2], px[2], "灰色图标像素 {i} R 不符");
            assert_eq!(got[3], px[3], "灰色图标像素 {i} alpha 不符");
            if px[3] != 0 {
                opaque.set(opaque.get() + 1);
                min_l = min_l.min(px[0]);
                max_l = max_l.max(px[0]);
            }
        }
        assert!(opaque.get() > 0, "灰色图标应存在不透明像素");
        // 亮度必须有对比度：全 0 说明加权或移位算错
        // （回归：Go 公式的 >>24 按 16 位色值设计，不能直接套用在 8 位输入上）
        assert!(max_l > 30, "灰度最大亮度 {max_l} 过低，灰度转换疑似有误");
        assert!(min_l < max_l, "灰度无对比度（min==max=={min_l}）");
        println!(
            "灰色图标不透明像素 {} 个，亮度范围 {min_l}..{max_l}",
            opaque.get()
        );
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

fn to_gray(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|px| {
            let (r, g, b, a) = (px[0] as u32, px[1] as u32, px[2] as u32, px[3]);
            // 与 Go 版 image.Gray 同一加权：0.299R + 0.587G + 0.114B。
            // Go 公式里的移位是 >>24，因为它的输入是 16 位色值；这里输入是
            // 8 位（= 16 位 ÷ 257 ≈ ÷ 256），除数相应降为 >>16，否则全图恒为 0。
            let l = ((19595 * r + 38470 * g + 7471 * b + (1 << 15)) >> 16) as u8;
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
