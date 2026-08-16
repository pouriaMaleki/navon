//! Persistent battery readout for the MAX17048 fuel gauge, drawn in
//! the top-right of the viewport on every frame.
//!
//! The 3.4C panel is round: the extreme screen corners sit outside the
//! visible circle, so the banner is placed well inside the inscribed
//! circle (right edge ≈ x 550 at this height) instead of hugging the
//! framebuffer corner. It lives in its own module so it can reuse the
//! same self-contained 5×7 glyph approach as `gps_overlay` without
//! growing that module's glyph table (which deliberately only covers
//! "GETTING GPS" + diagnostic counters).
//!
//! Text format: `82% 3.92V` — integer percent, cell voltage with two
//! decimals. When the gauge is absent or a bus read failed
//! (`FuelGaugeReading::present == false`) nothing is drawn — the
//! driver already logged the wiring problem, and fabricating a "0%"
//! would be worse than no readout.

use render_core::raster::Color;

use crate::framebuffer::RenderFramebuffer;
use crate::fuel_gauge::FuelGaugeReading;

/// Solid black banner background — high contrast against any map tile
/// underneath, no anti-alias needed.
const PANEL_BG: Color = Color::new(0x00, 0x00, 0x00);

/// Solid white text — readable in daylight against the black panel.
const TEXT_FG: Color = Color::new(0xFF, 0xFF, 0xFF);

/// Each glyph is 5 columns × 7 rows of bit cells.
const GLYPH_W: i32 = 5;
const GLYPH_H: i32 = 7;

/// Pixel size of one bit cell. The readout is a persistent HUD
/// element, not a splash — scale 3 keeps it unobtrusive while staying
/// legible at arm's length on the 800 px panel.
const SCALE_PX: i32 = 3;

/// Inter-character gap, in bit cells.
const KERN_CELLS: i32 = 1;

/// Padding (in pixels) between the text and the banner edge.
const PANEL_PAD_PX: i32 = 8;

/// Top margin. Keeps the banner clear of the panel's curved bezel.
const TOP_MARGIN_PX: i32 = 28;

/// Right edge of the banner, in pixels from the left of the
/// framebuffer. At y ≈ 28–70 the inscribed square of the 800 px
/// circle ends around x ≈ 550, so anchoring the banner's right edge
/// there keeps it fully visible on the round glass.
const RIGHT_EDGE_X: i32 = 550;

/// Format a reading as the overlay text: `82% 3.92V`. The voltage is
/// truncated (not rounded) to two decimals — a battery indicator does
/// not need banker's rounding.
pub fn format_battery_text(reading: FuelGaugeReading) -> String {
    format!(
        "{}% {}.{:02}V",
        reading.percent,
        reading.voltage_mv / 1000,
        (reading.voltage_mv % 1000) / 10,
    )
}

/// Render the corner readout into `fb`. No-op when the reading is
/// stale/absent. Caller is expected to have rendered the map (and any
/// center overlays) first; this only overwrites its own footprint.
pub fn render_battery_overlay(fb: &mut RenderFramebuffer, reading: FuelGaugeReading) {
    if !reading.present {
        return;
    }
    let text = format_battery_text(reading);
    let text_w =
        text.chars().count() as i32 * (GLYPH_W + KERN_CELLS) * SCALE_PX - KERN_CELLS * SCALE_PX;
    let text_h = GLYPH_H * SCALE_PX;

    let panel_w = text_w + PANEL_PAD_PX * 2;
    let panel_h = text_h + PANEL_PAD_PX * 2;
    let panel_x = RIGHT_EDGE_X - panel_w;
    let panel_y = TOP_MARGIN_PX;

    fb.fill_rect_overwrite(
        panel_x.max(0),
        panel_y.max(0),
        panel_w.max(0) as u32,
        panel_h.max(0) as u32,
        PANEL_BG,
    );

    let text_x = panel_x + PANEL_PAD_PX;
    let text_y = panel_y + PANEL_PAD_PX;
    draw_text(fb, &text, text_x, text_y, SCALE_PX, TEXT_FG);
}

fn draw_text(
    fb: &mut RenderFramebuffer,
    text: &str,
    x_origin: i32,
    y: i32,
    scale: i32,
    color: Color,
) {
    let glyph_w = GLYPH_W * scale;
    let kern = KERN_CELLS * scale;
    let mut cursor_x = x_origin;
    for ch in text.chars() {
        if ch == ' ' {
            cursor_x += glyph_w + kern;
            continue;
        }
        if let Some(rows) = glyph(ch) {
            draw_glyph(fb, rows, cursor_x, y, scale, color);
        }
        cursor_x += glyph_w + kern;
    }
}

fn draw_glyph(
    fb: &mut RenderFramebuffer,
    rows: [&'static str; GLYPH_H as usize],
    x: i32,
    y: i32,
    scale: i32,
    color: Color,
) {
    for (row_index, row) in rows.iter().enumerate() {
        for (col_index, cell) in row.chars().enumerate() {
            if cell != '1' {
                continue;
            }
            fb.fill_rect_overwrite(
                x + col_index as i32 * scale,
                y + row_index as i32 * scale,
                scale as u32,
                scale as u32,
                color,
            );
        }
    }
}

/// 5×7 bit-pattern glyphs covering the readout alphabet: decimal
/// digits plus `%`, `.` and `V`. Anything else renders as nothing.
fn glyph(ch: char) -> Option<[&'static str; GLYPH_H as usize]> {
    match ch {
        '0' => Some([
            "01110", "10001", "10011", "10101", "11001", "10001", "01110",
        ]),
        '1' => Some([
            "00100", "01100", "00100", "00100", "00100", "00100", "01110",
        ]),
        '2' => Some([
            "01110", "10001", "00001", "00010", "00100", "01000", "11111",
        ]),
        '3' => Some([
            "11110", "00001", "00001", "01110", "00001", "00001", "11110",
        ]),
        '4' => Some([
            "00010", "00110", "01010", "10010", "11111", "00010", "00010",
        ]),
        '5' => Some([
            "11111", "10000", "10000", "11110", "00001", "00001", "11110",
        ]),
        '6' => Some([
            "01110", "10000", "10000", "11110", "10001", "10001", "01110",
        ]),
        '7' => Some([
            "11111", "00001", "00010", "00100", "01000", "01000", "01000",
        ]),
        '8' => Some([
            "01110", "10001", "10001", "01110", "10001", "10001", "01110",
        ]),
        '9' => Some([
            "01110", "10001", "10001", "01111", "00001", "00001", "01110",
        ]),
        '%' => Some([
            "11001", "11011", "00010", "00100", "01000", "11011", "10011",
        ]),
        '.' => Some([
            "00000", "00000", "00000", "00000", "00000", "01100", "01100",
        ]),
        'V' => Some([
            "10001", "10001", "10001", "10001", "10001", "01010", "00100",
        ]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use render_core::raster::Framebuffer as RasterFramebuffer;

    fn sample_reading() -> FuelGaugeReading {
        FuelGaugeReading {
            percent: 82,
            voltage_mv: 3920,
            present: true,
        }
    }

    #[test]
    fn battery_text_formats_percent_and_two_decimal_volts() {
        assert_eq!(format_battery_text(sample_reading()), "82% 3.92V");
        assert_eq!(
            format_battery_text(FuelGaugeReading {
                percent: 100,
                voltage_mv: 4200,
                present: true,
            }),
            "100% 4.20V"
        );
        assert_eq!(
            format_battery_text(FuelGaugeReading {
                percent: 5,
                voltage_mv: 3090,
                present: true,
            }),
            "5% 3.09V"
        );
    }

    #[test]
    fn glyph_table_covers_every_character_of_the_readout_format() {
        for ch in format_battery_text(sample_reading())
            .chars()
            .filter(|c| *c != ' ')
        {
            assert!(
                glyph(ch).is_some(),
                "no glyph defined for '{ch}' but the readout format emits it"
            );
        }
    }

    #[test]
    fn overlay_writes_pixels_inside_the_visible_circle() {
        let mut fb: RenderFramebuffer = RasterFramebuffer::new(800, 800);
        let prefill = Color::new(0x10, 0x20, 0x30);
        fb.clear(prefill);
        render_battery_overlay(&mut fb, sample_reading());

        // The banner sits at y ≈ 28..70, x ≈ RIGHT_EDGE_X - width..
        // RIGHT_EDGE_X. Sample the middle of the banner: it must have
        // been overwritten with panel-black or text-white, and its
        // right edge must lie inside the round panel's circle
        // (x-400)^2 + (y-400)^2 <= 400^2.
        let pixels = fb.pixels();
        let w = fb.width() as usize;
        let mid_y = (TOP_MARGIN_PX + PANEL_PAD_PX + GLYPH_H * SCALE_PX / 2) as usize;
        let mid_x = (RIGHT_EDGE_X - 120) as usize;
        let idx = (mid_y * w + mid_x) * 4;
        let (r, g, b) = (pixels[idx], pixels[idx + 1], pixels[idx + 2]);
        let is_panel_or_text =
            (r == 0 && g == 0 && b == 0) || (r == 0xFF && g == 0xFF && b == 0xFF);
        assert!(
            is_panel_or_text,
            "banner center must be black or white; got ({r},{g},{b})"
        );

        // Right edge visibility check against the circle.
        let dx = (RIGHT_EDGE_X - 400) as i64;
        let dy = (mid_y as i64) - 400;
        assert!(
            dx * dx + dy * dy <= 400 * 400,
            "banner right edge ({RIGHT_EDGE_X},{mid_y}) falls outside the round panel"
        );
    }

    #[test]
    fn stale_reading_renders_nothing() {
        let mut fb: RenderFramebuffer = RasterFramebuffer::new(800, 800);
        let prefill = Color::new(0x10, 0x20, 0x30);
        fb.clear(prefill);
        render_battery_overlay(
            &mut fb,
            FuelGaugeReading {
                percent: 0,
                voltage_mv: 0,
                present: false,
            },
        );

        let pixels = fb.pixels();
        let w = fb.width() as usize;
        let mid_y = (TOP_MARGIN_PX + PANEL_PAD_PX + GLYPH_H * SCALE_PX / 2) as usize;
        let mid_x = (RIGHT_EDGE_X - 120) as usize;
        let idx = (mid_y * w + mid_x) * 4;
        assert_eq!(
            (pixels[idx], pixels[idx + 1], pixels[idx + 2]),
            (0x10, 0x20, 0x30),
            "stale reading must leave the framebuffer untouched"
        );
    }
}
