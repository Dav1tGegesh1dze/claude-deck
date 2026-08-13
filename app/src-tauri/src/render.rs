//! Turns a usage percentage into a button-sized image: a fill bar colored
//! by Anthropic's own `severity` field (SPEC.md §5 — we trust their
//! severity over inventing our own thresholds), a short metric-kind label
//! ("5H"/"7D"/"BUD" — added after hardware testing showed session/weekly
//! were visually indistinguishable), and the percentage as text.
//!
//! Font: bundled Roboto (OFL, see assets/Roboto-OFL.txt), variable font
//! rendered at its default instance.

use ab_glyph::{FontArc, PxScale};
use anyhow::{anyhow, Result};
use image::{imageops::FilterType, DynamicImage, Rgb, RgbImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use std::path::Path;
use std::sync::LazyLock;

static FONT_BYTES: &[u8] = include_bytes!("../assets/Roboto-Variable.ttf");

static FONT: LazyLock<FontArc> =
    LazyLock::new(|| FontArc::try_from_slice(FONT_BYTES).expect("bundled font is valid"));

fn severity_color(severity: &str) -> Rgb<u8> {
    match severity {
        "critical" => Rgb([216, 58, 58]),
        "warning" => Rgb([216, 164, 0]),
        "normal" => Rgb([46, 158, 76]),
        "off" => Rgb([55, 55, 65]),
        _ => Rgb([100, 100, 100]),
    }
}

const BACKGROUND: Rgb<u8> = Rgb([20, 20, 20]);
const TEXT_COLOR: Rgb<u8> = Rgb([255, 255, 255]);
const LABEL_COLOR: Rgb<u8> = Rgb([210, 210, 210]);

fn center_x(text: &str, scale: PxScale, width: u32) -> i32 {
    let estimate = text.len() as f32 * scale.x * 0.55;
    ((width as f32 - estimate) / 2.0).max(0.0) as i32
}

fn render_on(
    mut img: RgbImage,
    label: &str,
    percent: f64,
    severity: &str,
    main_text_override: Option<&str>,
) -> DynamicImage {
    let (width, height) = img.dimensions();

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

    let label_scale = PxScale::from(height as f32 * 0.2);
    let label_x = center_x(label, label_scale, width);
    draw_text_mut(&mut img, LABEL_COLOR, label_x, 2, label_scale, &*FONT, label);

    let main_text = main_text_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}", percent.round() as i64));
    let main_scale = PxScale::from(height as f32 * 0.4);
    let main_x = center_x(&main_text, main_scale, width);
    let main_y = 2 + label_scale.y as i32 + 2;
    draw_text_mut(
        &mut img,
        TEXT_COLOR,
        main_x,
        main_y,
        main_scale,
        &*FONT,
        &main_text,
    );

    DynamicImage::ImageRgb8(img)
}

/// Renders a percentage + severity as a button image on a solid
/// background, with a short label above it (e.g. "5H", "7D", "BUD") so
/// buttons showing different metrics are visually distinguishable.
/// `width`/`height` should come from the target device's
/// `ImageFormat::size` — mirajazz resizes on send regardless, but
/// rendering close to native size keeps the small text legible.
pub fn render_percent(
    label: &str,
    percent: f64,
    severity: &str,
    width: u32,
    height: u32,
) -> DynamicImage {
    render_on(
        RgbImage::from_pixel(width, height, BACKGROUND),
        label,
        percent,
        severity,
        None,
    )
}

/// Same as [render_percent], but drawn over a user-supplied custom icon
/// image instead of a solid background (SPEC.md §6.2).
pub fn render_percent_on_background(
    background_path: &Path,
    label: &str,
    percent: f64,
    severity: &str,
    width: u32,
    height: u32,
) -> Result<DynamicImage> {
    let base = image::open(background_path)
        .map_err(|e| anyhow!("failed to load icon image {}: {e}", background_path.display()))?
        .resize_exact(width, height, FilterType::Lanczos3)
        .into_rgb8();

    Ok(render_on(base, label, percent, severity, None))
}

/// A metric assigned to a button but not currently active (e.g. the
/// self-imposed budget metric is assigned but budget tracking is
/// disabled in settings) — shows "OFF" instead of silently pushing
/// nothing, so the user gets feedback instead of a blank/stale button.
pub fn render_disabled(label: &str, width: u32, height: u32) -> DynamicImage {
    render_on(
        RgbImage::from_pixel(width, height, BACKGROUND),
        label,
        0.0,
        "off",
        Some("OFF"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_at_requested_size() {
        let img = render_percent("7D", 94.0, "critical", 60, 60);
        assert_eq!((img.width(), img.height()), (60, 60));
    }

    #[test]
    fn zero_percent_has_no_fill_bar() {
        let img = render_percent("5H", 0.0, "normal", 60, 60).into_rgb8();
        // Bottom-left pixel should still be background, not fill color.
        assert_eq!(*img.get_pixel(0, 59), BACKGROUND);
    }

    #[test]
    fn high_percent_fills_bottom_row() {
        let img = render_percent("7D", 100.0, "critical", 60, 60).into_rgb8();
        assert_eq!(*img.get_pixel(0, 59), severity_color("critical"));
    }

    #[test]
    fn clamps_out_of_range_percent() {
        // Should not panic on bad input from an unexpected API shape.
        let _ = render_percent("5H", 150.0, "normal", 60, 60);
        let _ = render_percent("5H", -10.0, "normal", 60, 60);
    }

    #[test]
    fn disabled_state_renders_without_panicking() {
        let img = render_disabled("BUD", 60, 60);
        assert_eq!((img.width(), img.height()), (60, 60));
    }
}
