//! Turns a usage percentage into a button-sized image: a fill bar colored
//! by Anthropic's own `severity` field (SPEC.md §5 — we trust their
//! severity over inventing our own thresholds) plus the percentage as text.
//!
//! Font: bundled Roboto (OFL, see assets/Roboto-OFL.txt), variable font
//! rendered at its default instance.

use ab_glyph::{FontArc, PxScale};
use image::{DynamicImage, Rgb, RgbImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use std::sync::LazyLock;

static FONT_BYTES: &[u8] = include_bytes!("../assets/Roboto-Variable.ttf");

static FONT: LazyLock<FontArc> =
    LazyLock::new(|| FontArc::try_from_slice(FONT_BYTES).expect("bundled font is valid"));

fn severity_color(severity: &str) -> Rgb<u8> {
    match severity {
        "critical" => Rgb([216, 58, 58]),
        "warning" => Rgb([216, 164, 0]),
        "normal" => Rgb([46, 158, 76]),
        _ => Rgb([100, 100, 100]),
    }
}

const BACKGROUND: Rgb<u8> = Rgb([20, 20, 20]);
const TEXT_COLOR: Rgb<u8> = Rgb([255, 255, 255]);

/// Renders a percentage + severity as a button image. `width`/`height`
/// should come from the target device's `ImageFormat::size` — mirajazz
/// resizes on send regardless, but rendering close to native size keeps
/// the small text legible.
pub fn render_percent(percent: f64, severity: &str, width: u32, height: u32) -> DynamicImage {
    let mut img = RgbImage::from_pixel(width, height, BACKGROUND);

    let percent = percent.clamp(0.0, 100.0);
    let fill_height = ((percent / 100.0) * height as f64).round() as i32;
    let color = severity_color(severity);

    if fill_height > 0 {
        draw_filled_rect_mut(
            &mut img,
            Rect::at(0, height as i32 - fill_height).of_size(width, fill_height as u32),
            color,
        );
    }

    let label = format!("{}", percent.round() as i64);
    let scale = PxScale::from(height as f32 * 0.42);
    // Rough monospace-ish width estimate to center the label; good enough
    // at this size, not worth measuring exact glyph metrics.
    let text_width_estimate = label.len() as f32 * scale.x * 0.55;
    let x = ((width as f32 - text_width_estimate) / 2.0).max(0.0) as i32;
    let y = ((height as f32 - scale.y) / 2.0) as i32;

    draw_text_mut(&mut img, TEXT_COLOR, x, y, scale, &*FONT, &label);

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_at_requested_size() {
        let img = render_percent(94.0, "critical", 60, 60);
        assert_eq!((img.width(), img.height()), (60, 60));
    }

    #[test]
    fn zero_percent_has_no_fill_bar() {
        let img = render_percent(0.0, "normal", 60, 60).into_rgb8();
        // Bottom-left pixel should still be background, not fill color.
        assert_eq!(*img.get_pixel(0, 59), BACKGROUND);
    }

    #[test]
    fn high_percent_fills_bottom_row() {
        let img = render_percent(100.0, "critical", 60, 60).into_rgb8();
        assert_eq!(*img.get_pixel(0, 59), severity_color("critical"));
    }

    #[test]
    fn clamps_out_of_range_percent() {
        // Should not panic on bad input from an unexpected API shape.
        let _ = render_percent(150.0, "normal", 60, 60);
        let _ = render_percent(-10.0, "normal", 60, 60);
    }
}
