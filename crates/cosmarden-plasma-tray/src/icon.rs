//! Contrast tray pixmaps. Dark panel → light (`white*`) glyph; light panel →
//! dark (`black*`) glyph. Locked uses the `*_locked` siblings.
//!
//! Same four PNGs the libcosmic tray embeds (`icons/`). Plasma does not recolor
//! a named symbolic icon, so the contrast is baked into the pixmap and the
//! panel's darkness has to be read at startup (`crate::theme`).

use ksni::Icon;

const WHITE: &[u8] = include_bytes!("../../../icons/white32.png");
const BLACK: &[u8] = include_bytes!("../../../icons/black32.png");
const WHITE_LOCKED: &[u8] = include_bytes!("../../../icons/white_locked32.png");
const BLACK_LOCKED: &[u8] = include_bytes!("../../../icons/black_locked32.png");

/// PNG bytes for a panel that is `dark` and a vault that is `locked`.
pub fn png_for(dark_panel: bool, locked: bool) -> &'static [u8] {
    match (dark_panel, locked) {
        (true, false) => WHITE,
        (true, true) => WHITE_LOCKED,
        (false, false) => BLACK,
        (false, true) => BLACK_LOCKED,
    }
}

/// The SNI pixmap for this panel and lock state.
pub fn sni_icon(dark_panel: bool, locked: bool) -> Icon {
    decode_argb32(png_for(dark_panel, locked))
}

fn decode_argb32(png: &[u8]) -> Icon {
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().expect("tray png header");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("tray png frame");
    let mut data = buf[..info.buffer_size()].to_vec();
    assert_eq!(
        info.color_type,
        png::ColorType::Rgba,
        "tray icons are RGBA PNGs"
    );
    // ksni wants ARGB32 (see ksni::Icon example: rotate RGBA → ARGB).
    for pixel in data.as_chunks_mut::<4>().0 {
        pixel.rotate_right(1);
    }
    Icon {
        width: info.width as i32,
        height: info.height as i32,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_panel_uses_the_light_glyph() {
        assert_eq!(png_for(true, false), WHITE);
        assert_eq!(png_for(true, true), WHITE_LOCKED);
        assert_eq!(png_for(false, false), BLACK);
        assert_eq!(png_for(false, true), BLACK_LOCKED);
    }

    /// Every state decodes to a full ARGB32 buffer: a truncated PNG or a
    /// non-RGBA colour type must fail loudly here, not draw a blank icon.
    #[test]
    fn every_icon_decodes_to_argb32() {
        for (dark, locked) in [(true, false), (true, true), (false, false), (false, true)] {
            let icon = sni_icon(dark, locked);
            assert!(icon.width > 0 && icon.height > 0);
            assert_eq!(
                icon.data.len(),
                (icon.width * icon.height * 4) as usize,
                "dark={dark} locked={locked}"
            );
        }
    }

    /// A dark panel must get the light glyph: the average alpha-weighted
    /// brightness of the visible pixels is what the eye sees on the panel.
    #[test]
    fn a_dark_panel_gets_a_light_glyph() {
        let brightness = |icon: &Icon| {
            let mut sum = 0u64;
            let mut count = 0u64;
            for pixel in icon.data.as_chunks::<4>().0 {
                // ARGB32 (byte order after the rotate): a, r, g, b.
                let (r, g, b) = (pixel[1] as u64, pixel[2] as u64, pixel[3] as u64);
                if b + g + r > 0 {
                    sum += r + g + b;
                    count += 1;
                }
            }
            sum / count.max(1)
        };
        assert!(
            brightness(&sni_icon(true, false)) > brightness(&sni_icon(false, false)),
            "the glyph for a dark panel must be lighter than for a light panel"
        );
    }
}
