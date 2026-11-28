// 托盘图标：内嵌 PNG 解码成 32 位 HICON。
// 样式集合：0 = 键盘板（默认，绿/珊瑚），1 = 脉冲屏（标准版 icon-switch feature）。

use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

/// 每套样式一对 (normal, paused) HICON；程序退出时随进程一并释放（托盘常驻，无需显式销毁）。
pub(crate) struct Icons {
    /// 精简版（无 icon-switch feature）只有第一套
    pub sets: Vec<(HICON, HICON)>,
}

impl Icons {
    /// 配置选择的样式下标，越界时退回最后一套（兼容精简版读到大样式号）
    pub fn pick(&self, style: u8) -> (HICON, HICON) {
        let idx = (style as usize).min(self.sets.len() - 1);
        self.sets[idx]
    }

    /// 是否提供多套样式（决定菜单是否显示切换入口）
    pub fn switchable(&self) -> bool {
        self.sets.len() > 1
    }
}

fn pair_from_pngs(normal_png: &[u8], paused_png: &[u8]) -> Result<(HICON, HICON), String> {
    let (w, h, rgba) = crate::assets::decode_rgba8(normal_png)?;
    let normal = icon_from_rgba(w, h, &rgba)?;
    let (w, h, rgba) = crate::assets::decode_rgba8(paused_png)?;
    let paused = icon_from_rgba(w, h, &rgba)?;
    Ok((normal, paused))
}

/// 从内嵌 PNG 解码并生成全部样式的 HICON；解码失败返回错误文本。
pub(crate) fn load(normal_png: &[u8], paused_png: &[u8]) -> Result<Icons, String> {
    let first = pair_from_pngs(normal_png, paused_png)?;
    #[cfg(feature = "icon-switch")]
    let alt = Some(pair_from_pngs(
        crate::assets::ICON_ALT_PNG,
        crate::assets::ICON_ALT_PAUSED_PNG,
    )?);
    #[cfg(not(feature = "icon-switch"))]
    let alt: Option<(HICON, HICON)> = None;
    let sets: Vec<_> = [Some(first), alt].into_iter().flatten().collect();
    Ok(Icons { sets })
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
        let (normal, paused) = icons.sets[0];
        assert!(!normal.0.is_null());
        assert!(!paused.0.is_null());
        // 标准版（默认 features）有两套样式且互不相等；精简版编译时只有一套
        assert_eq!(icons.sets.len(), if cfg!(feature = "icon-switch") { 2 } else { 1 });
        assert_eq!(icons.pick(0).0, icons.sets[0].0);
        assert_eq!(icons.pick(9).0, icons.sets[icons.sets.len() - 1].0, "越界样式号应收敛");
        assert_eq!(icons.switchable(), cfg!(feature = "icon-switch"));
    }

    // 读出 HICON 的 32 位 BGRA 像素（自顶向下），验证两枚图标都与内嵌 PNG 逐像素一致。
    #[test]
    fn icon_pixels_match_embedded_png() {
        let (w, h, normal_rgba) = crate::assets::decode_rgba8(crate::assets::ICON_PNG).unwrap();
        let (_, _, paused_rgba) = crate::assets::decode_rgba8(crate::assets::ICON_PAUSED_PNG).unwrap();
        let icons = load(crate::assets::ICON_PNG, crate::assets::ICON_PAUSED_PNG).unwrap();
        let (normal_icon, paused_icon) = icons.sets[0];

        let normal_px = icon_pixels(normal_icon, w, h);
        let paused_px = icon_pixels(paused_icon, w, h);
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

    #[cfg(feature = "icon-switch")]
    #[test]
    fn alt_icon_pixels_match_embedded_png() {
        let (w, h, normal_rgba) = crate::assets::decode_rgba8(crate::assets::ICON_ALT_PNG).unwrap();
        let (_, _, paused_rgba) = crate::assets::decode_rgba8(crate::assets::ICON_ALT_PAUSED_PNG).unwrap();
        let icons = load(crate::assets::ICON_PNG, crate::assets::ICON_PAUSED_PNG).unwrap();
        let (normal_icon, paused_icon) = icons.pick(1);
        assert_eq!(icons.pick(1).0, icons.sets[1].0, "样式 1 应选中脉冲屏套图");

        for (name, rgba, icon) in [
            ("彩色", &normal_rgba, normal_icon),
            ("暂停", &paused_rgba, paused_icon),
        ] {
            let pxs = icon_pixels(icon, w, h);
            for (i, px) in rgba.chunks_exact(4).enumerate() {
                let got = &pxs[i * 4..i * 4 + 4];
                assert_eq!(got[0], px[2], "脉冲屏{name}图标像素 {i} B 通道不符");
                assert_eq!(got[1], px[1], "脉冲屏{name}图标像素 {i} G 通道不符");
                assert_eq!(got[2], px[0], "脉冲屏{name}图标像素 {i} R 通道不符");
                assert_eq!(got[3], px[3], "脉冲屏{name}图标像素 {i} alpha 不符");
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
