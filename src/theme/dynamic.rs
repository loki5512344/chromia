//! Dynamic palette extraction from cover-art image bytes.
//!
//! Decodes the image, quantizes it down to a handful of colors with
//! color-thief, picks the most saturated viable color as the accent, then
//! derives the rest of the palette from it via manual HSL math (no palette
//! crate needed).

use std::io::Cursor;

use color_thief::{Color, ColorFormat};
use image::ImageReader;

use crate::theme::Palette;

/// Extracts a resolved palette from cover-art bytes (PNG/JPEG/WebP).
///
/// Falls back to an error when the image cannot be decoded or quantized;
/// callers are expected to fall back to the Catppuccin palette.
pub fn palette_from_image(image_bytes: &[u8]) -> anyhow::Result<Palette> {
    let reader = ImageReader::new(Cursor::new(image_bytes))
        .with_guessed_format()
        .map_err(|e| anyhow::anyhow!("could not guess image format: {e}"))?;
    let img = reader
        .decode()
        .map_err(|e| anyhow::anyhow!("could not decode image: {e}"))?;
    let rgb = img.thumbnail(32, 32).to_rgb8();
    let raw = rgb.as_raw();

    let colors = color_thief::get_palette(raw, ColorFormat::Rgb, 10, 5)
        .map_err(|e| anyhow::anyhow!("color quantization failed: {e:?}"))?;

    let accent = pick_accent(&colors);
    let background = hsl_set_lightness(accent, 0.06);
    let surface = hsl_set_lightness(accent, 0.09);
    let overlay = hsl_set_lightness(accent, 0.13);
    let text = text_for_background(background);
    let subtext = blend(text, background, 0.6);

    Ok(Palette {
        background: rgb_to_hex(background),
        surface: rgb_to_hex(surface),
        overlay: rgb_to_hex(overlay),
        accent: rgb_to_hex(accent),
        text: rgb_to_hex(text),
        subtext: rgb_to_hex(subtext),
    })
}

/// Picks the accent color: the most saturated palette entry whose perceived
/// luminance sits in a usable band. If every entry is too dark or too bright,
/// falls back to the brightest one.
fn pick_accent(colors: &[Color]) -> [u8; 3] {
    let viable = colors
        .iter()
        .filter(|c| (0.15..=0.85).contains(&luminance([c.r, c.g, c.b])))
        .max_by(|a, b| saturation([a.r, a.g, a.b]).total_cmp(&saturation([b.r, b.g, b.b])));
    viable
        .map(|c| [c.r, c.g, c.b])
        .or_else(|| {
            colors
                .iter()
                .max_by(|a, b| luminance([a.r, a.g, a.b]).total_cmp(&luminance([b.r, b.g, b.b])))
                .map(|c| [c.r, c.g, c.b])
        })
        .unwrap_or([0xcb, 0xa6, 0xf7])
}

/// Near-white text for dark backgrounds, near-black for light ones.
fn text_for_background(background: [u8; 3]) -> [u8; 3] {
    if luminance(background) < 0.5 {
        [0xcd, 0xd6, 0xf4]
    } else {
        [0x4c, 0x4f, 0x69]
    }
}

/// Perceived (linear-ish) luminance on `[0, 1]` using Rec. 709 weights.
fn luminance([r, g, b]: [u8; 3]) -> f64 {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Normalized saturation (`max - min`) of the RGB channels.
fn saturation([r, g, b]: [u8; 3]) -> f64 {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    r.max(g).max(b) - r.min(g).min(b)
}

/// Sets the HSL lightness of a color, keeping hue and saturation intact.
fn hsl_set_lightness(color: [u8; 3], lightness: f64) -> [u8; 3] {
    let (h, s, _) = rgb_to_hsl(color);
    hsl_to_rgb(h, s, lightness)
}

/// Converts RGB (8-bit channels) to `(hue, saturation, lightness)` with
/// `hue` in degrees `[0, 360)` and `saturation`/`lightness` in `[0, 1]`.
fn rgb_to_hsl([r, g, b]: [u8; 3]) -> (f64, f64, f64) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d <= f64::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

/// Converts `(hue, saturation, lightness)` back to 8-bit RGB.
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let (r, g, b) = ((r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0);
    [
        r.round().clamp(0.0, 255.0) as u8,
        g.round().clamp(0.0, 255.0) as u8,
        b.round().clamp(0.0, 255.0) as u8,
    ]
}

/// Alpha-composites `fg` over `bg` at the given `opacity`.
fn blend(fg: [u8; 3], bg: [u8; 3], opacity: f64) -> [u8; 3] {
    let mix = |f: u8, b: u8| (f as f64 * opacity + b as f64 * (1.0 - opacity)).round() as u8;
    [mix(fg[0], bg[0]), mix(fg[1], bg[1]), mix(fg[2], bg[2])]
}

/// Formats an RGB triplet as a lowercase `#rrggbb` hex string.
fn rgb_to_hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes a solid-color 64x64 PNG in memory.
    fn solid_png(color: [u8; 3]) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(64, 64, image::Rgb(color));
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .expect("png encodes in memory");
        bytes
    }

    fn hex_to_rgb(hex: &str) -> [u8; 3] {
        let h = hex.trim_start_matches('#');
        [
            u8::from_str_radix(&h[0..2], 16).expect("valid hex"),
            u8::from_str_radix(&h[2..4], 16).expect("valid hex"),
            u8::from_str_radix(&h[4..6], 16).expect("valid hex"),
        ]
    }

    #[test]
    fn rgb_to_hex_formats_lowercase() {
        assert_eq!(rgb_to_hex([0x1e, 0x1e, 0x2e]), "#1e1e2e");
        assert_eq!(rgb_to_hex([0xff, 0x80, 0x00]), "#ff8000");
    }

    #[test]
    fn palette_from_image_uses_dominant_color_as_accent() {
        let png = solid_png([200, 120, 240]);
        let palette = palette_from_image(&png).expect("extracts a palette");
        let accent = hex_to_rgb(&palette.accent);
        for (got, want) in accent.into_iter().zip([200, 120, 240]) {
            assert!(
                (got as i16 - want as i16).unsigned_abs() <= 30,
                "accent channel {got} not close to {want}"
            );
        }
        assert!(palette.background.starts_with('#'));
        assert!(palette.background.len() == 7);
    }

    #[test]
    fn background_is_darkened_accent() {
        let dark = hsl_set_lightness([200, 120, 240], 0.06);
        let (_, _, l) = rgb_to_hsl(dark);
        assert!((l - 0.06).abs() < 0.01);
    }

    #[test]
    fn text_choice_follows_background_luminance() {
        assert_eq!(text_for_background([0x1e, 0x1e, 0x2e]), [0xcd, 0xd6, 0xf4]);
        assert_eq!(text_for_background([0xe6, 0xe9, 0xef]), [0x4c, 0x4f, 0x69]);
    }

    #[test]
    fn hsl_roundtrip_preserves_pure_red() {
        assert_eq!(hsl_set_lightness([255, 0, 0], 0.5), [255, 0, 0]);
    }

    #[test]
    fn invalid_bytes_return_an_error() {
        assert!(palette_from_image(b"definitely not an image").is_err());
    }
}
