// 编译期内嵌的静态资源。

/// 托盘图标使用的 PNG 母版（256x256）。
pub const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

/// 暂停态托盘图标的 PNG 母版（256x256，珊瑚色变体）。
pub const ICON_PAUSED_PNG: &[u8] = include_bytes!("../assets/icon-paused.png");

/// 第二套图标样式「脉冲屏」（标准版 icon-switch feature 才内嵌，精简版为零成本）。
#[cfg(feature = "icon-switch")]
pub const ICON_ALT_PNG: &[u8] = include_bytes!("../assets/icon-alt.png");
#[cfg(feature = "icon-switch")]
pub const ICON_ALT_PAUSED_PNG: &[u8] = include_bytes!("../assets/icon-alt-paused.png");

// PNG 解码到 RGBA8：两个平台的托盘图标共用。
//
// 注意 png crate 自 0.17 起默认变换为 IDENTITY，调色板图会原样输出索引字节；
// 必须显式请求 ALPHA（隐含 EXPAND，把调色板/tRNS 展开成 RGBA）与 STRIP_16。

/// 解码 PNG 字节为 RGBA8，返回 (宽, 高, 像素)。
pub fn decode_rgba8(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::ALPHA | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("解析图标失败：{e}"))?;
    let (color, _depth) = reader.output_color_type();
    let info = reader.info().clone();
    let (w, h) = (info.width, info.height);

    let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or_default()];
    let out = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("解码图标失败：{e}"))?;
    let data = &buf[..out.buffer_size()];

    let expected = w as usize * h as usize;
    let rgba = match color {
        png::ColorType::Rgba => {
            if data.len() != expected * 4 {
                return Err(format!("图标解码长度异常（{w}x{h}，{} 字节）", data.len()));
            }
            data.to_vec()
        }
        png::ColorType::Rgb => {
            if data.len() != expected * 3 {
                return Err(format!("图标解码长度异常（{w}x{h}，{} 字节）", data.len()));
            }
            data.chunks_exact(3)
                .flat_map(|px| [px[0], px[1], px[2], 255])
                .collect()
        }
        png::ColorType::Grayscale => {
            if data.len() != expected {
                return Err(format!("图标解码长度异常（{w}x{h}，{} 字节）", data.len()));
            }
            data.iter().flat_map(|&l| [l, l, l, 255]).collect()
        }
        png::ColorType::GrayscaleAlpha => {
            if data.len() != expected * 2 {
                return Err(format!("图标解码长度异常（{w}x{h}，{} 字节）", data.len()));
            }
            data.chunks_exact(2)
                .flat_map(|px| [px[0], px[0], px[0], px[1]])
                .collect()
        }
        other => return Err(format!("图标颜色格式不受支持（{other:?}）")),
    };
    Ok((w, h, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_icon_decodes_to_rgba8() {
        let (w, h, rgba) = decode_rgba8(ICON_PNG).expect("内嵌图标应能解码为 RGBA8");
        assert_eq!((w, h), (256, 256));
        assert_eq!(rgba.len(), 256 * 256 * 4);
        // 图标带 tRNS：既有不透明像素也有全透明像素
        let mut transparent = 0;
        let mut opaque = 0;
        for px in rgba.chunks_exact(4) {
            match px[3] {
                0 => transparent += 1,
                255 => opaque += 1,
                _ => {}
            }
        }
        assert!(transparent > 0, "应有透明像素");
        assert!(opaque > 0, "应有不透明像素");
    }
}
