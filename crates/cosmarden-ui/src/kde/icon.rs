//! Contrast tray pixmaps. Dark panel → light (`white*`) glyph; light panel
//! → dark (`black*`) glyph. Locked uses the `*_locked` siblings.

use ksni::Icon;

const WHITE: &[u8] = include_bytes!("../../../../icons/white32.png");
const BLACK: &[u8] = include_bytes!("../../../../icons/black32.png");
const WHITE_LOCKED: &[u8] = include_bytes!("../../../../icons/white_locked32.png");
const BLACK_LOCKED: &[u8] = include_bytes!("../../../../icons/black_locked32.png");

/// PNG bytes for a panel that is `dark` and a vault that is `locked`.
pub fn png_for(dark_panel: bool, locked: bool) -> &'static [u8] {
    match (dark_panel, locked) {
        (true, false) => WHITE,
        (true, true) => WHITE_LOCKED,
        (false, false) => BLACK,
        (false, true) => BLACK_LOCKED,
    }
}

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

    #[test]
    fn decoded_white_icon_is_light_on_average() {
        let icon = decode_argb32(WHITE);
        assert_eq!(icon.width, 32);
        assert_eq!(icon.height, 32);
        assert_eq!(icon.data.len(), 32 * 32 * 4);
        // ARGB after rotate: [A, R, G, B]. Opaque-enough pixels of the
        // "white" mark must be light so they contrast on a dark panel.
        let mut lum_sum = 0u64;
        let mut n = 0u64;
        for px in icon.data.as_chunks::<4>().0 {
            if px[0] > 32 {
                lum_sum += u64::from(px[1]) + u64::from(px[2]) + u64::from(px[3]);
                n += 1;
            }
        }
        assert!(n > 0, "white32.png must have visible pixels");
        let mean = lum_sum / n / 3;
        assert!(
            mean > 160,
            "white tray glyph mean RGB {mean} is too dark for a dark panel"
        );
    }

    #[test]
    fn decoded_black_icon_is_dark_on_average() {
        let icon = decode_argb32(BLACK);
        let mut lum_sum = 0u64;
        let mut n = 0u64;
        for px in icon.data.as_chunks::<4>().0 {
            if px[0] > 32 {
                lum_sum += u64::from(px[1]) + u64::from(px[2]) + u64::from(px[3]);
                n += 1;
            }
        }
        assert!(n > 0);
        let mean = lum_sum / n / 3;
        assert!(
            mean < 96,
            "black tray glyph mean RGB {mean} is too light for a light panel"
        );
    }
}
