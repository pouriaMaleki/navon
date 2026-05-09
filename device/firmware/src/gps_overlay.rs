//! "GETTING GPS" splash drawn on top of the map while we're still waiting
//! for the NEO-6M's first valid fix.
//!
//! The map renders normally underneath (camera held on the map's centroid
//! by [`crate::gps::SeedThenRealGpsProvider`]) so the rider has spatial
//! context — the panel here is a centered black banner with white text
//! sitting over the basemap. As soon as `EspIdfGpsProvider` reports its
//! first real fix the overlay disappears and the rider's marker takes
//! over.
//!
//! Self-contained: 5×7 bit-pattern font, no dependency on render-core's
//! private `banner_glyph` table (which doesn't carry P or S, the two
//! letters a "GPS" string actually needs). Pixel-write goes through the
//! framebuffer's existing `fill_rect_overwrite`, so this works
//! identically on host (RGBA) and device (RGB565) builds.

use render_core::raster::Color;

use crate::framebuffer::RenderFramebuffer;
use crate::gps::GpsDiagnostics;

/// Solid black banner background — high contrast against any map tile
/// underneath, no anti-alias needed.
const PANEL_BG: Color = Color::new(0x00, 0x00, 0x00);

/// Solid white text — readable in daylight against the black panel.
const TEXT_FG: Color = Color::new(0xFF, 0xFF, 0xFF);

/// Each glyph is 5 columns × 7 rows of bit cells.
const GLYPH_W: i32 = 5;
const GLYPH_H: i32 = 7;

/// Pixel size of one bit cell. Panel diameter is 800 px on the 3.4C; at
/// scale 6 the "GETTING GPS" string is ~360 px wide which fits inside
/// the round display's inscribed square with comfortable margin.
const SCALE_PX: i32 = 6;

/// Inter-character gap, also in bit cells (so it scales with `SCALE_PX`).
const KERN_CELLS: i32 = 1;

/// Pixel size of one bit cell on the diagnostic counter lines drawn
/// below the main banner. Smaller than the banner so a `B 1234567 S
/// 12345 F 1234` counter row fits inside the inscribed square.
const DIAG_SCALE_PX: i32 = 4;

/// Vertical gap (in pixels) between the banner and the first counter
/// line, and between successive counter lines.
const DIAG_LINE_GAP_PX: i32 = 12;

/// Padding (in pixels) between the text bounding box and the panel edge.
const PANEL_PAD_PX: i32 = 24;

/// String shown while we wait for the first real GPS fix.
pub const ACQUIRING_TEXT: &str = "GETTING GPS";

/// Format a diagnostics row in HUD style: a 4-letter mnemonic followed
/// by a 6-digit hex counter prefixed `0x`. Hex makes the steady-state
/// stream of NMEA bytes look less like a runaway odometer and more
/// like the kind of telemetry you'd expect on a sci-fi flight HUD.
/// We saturate the value at `0xFF_FFFF` (16M − 1) because the device
/// counters can in principle exceed that after long uptime — the
/// counter display tops out and stops growing visually.
fn format_diag_row(label: &str, value: u64) -> String {
    let clamped = value.min(0xFF_FFFF);
    format!("{label} 0x{clamped:06X}")
}

/// Render a centered "GETTING GPS" banner into `fb`. If the platform
/// has handed up live `diagnostics`, three counter rows are drawn
/// below the banner so an operator in the field can tell *why* GPS
/// hasn't acquired yet:
///
/// * `B <bytes_seen>` — bytes pulled off the UART RX FIFO. Stuck at
///   `0` ⇒ wiring is wrong (TX/RX swapped, wrong GPIO, no power on
///   the module). Counting up but `S` stuck at `0` ⇒ baud rate is
///   wrong.
/// * `S <sentences_seen>` — `\n`-delimited NMEA sentences the parser
///   pulled out. Counting up but `F` stuck at `0` ⇒ the module is
///   alive and talking but hasn't locked any satellites yet (warm-up
///   period, antenna problem, indoor or shielded sky view).
/// * `F <fixes_seen>` — valid `RMC` fixes. As soon as this goes
///   non-zero the platform layer flips
///   [`crate::app::App::set_gps_acquired(true)`] and the whole
///   overlay disappears on the next frame.
///
/// Caller is expected to have already rendered the map underneath
/// this frame; the panel only overwrites its own footprint and leaves
/// the rest of the basemap visible.
pub fn render_acquiring_gps_overlay(
    fb: &mut RenderFramebuffer,
    diagnostics: Option<GpsDiagnostics>,
) {
    let fb_w = fb.width() as i32;
    let fb_h = fb.height() as i32;

    // Banner geometry.
    let banner_glyph_count = ACQUIRING_TEXT.chars().count() as i32;
    let banner_glyph_width = GLYPH_W * SCALE_PX;
    let banner_kern = KERN_CELLS * SCALE_PX;
    let banner_text_w =
        banner_glyph_count * banner_glyph_width + (banner_glyph_count - 1).max(0) * banner_kern;
    let banner_text_h = GLYPH_H * SCALE_PX;

    // Diagnostic-row geometry (only relevant when `diagnostics` is Some).
    //
    // HUD-style mnemonics:
    //   * BITS — raw byte count off the UART data link
    //   * PING — NMEA-0183 sentences successfully decoded
    //   * PINS — valid position fixes parsed from RMC sentences
    let diag_rows: Vec<String> = match diagnostics {
        Some(diag) => vec![
            format_diag_row("BITS", diag.bytes_seen),
            format_diag_row("PING", diag.sentences_seen),
            format_diag_row("PINS", diag.fixes_seen),
        ],
        None => Vec::new(),
    };
    let diag_glyph_width = GLYPH_W * DIAG_SCALE_PX;
    let diag_kern = KERN_CELLS * DIAG_SCALE_PX;
    let diag_row_h = GLYPH_H * DIAG_SCALE_PX;
    let diag_widest_row = diag_rows
        .iter()
        .map(|row| {
            let n = row.chars().count() as i32;
            n * diag_glyph_width + (n - 1).max(0) * diag_kern
        })
        .max()
        .unwrap_or(0);
    let diag_block_h = if diag_rows.is_empty() {
        0
    } else {
        DIAG_LINE_GAP_PX
            + diag_rows.len() as i32 * diag_row_h
            + (diag_rows.len() as i32 - 1).max(0) * DIAG_LINE_GAP_PX
    };

    let content_w = banner_text_w.max(diag_widest_row);
    let content_h = banner_text_h + diag_block_h;
    let panel_w = content_w + PANEL_PAD_PX * 2;
    let panel_h = content_h + PANEL_PAD_PX * 2;
    let panel_x = (fb_w - panel_w) / 2;
    let panel_y = (fb_h - panel_h) / 2;

    fb.fill_rect_overwrite(
        panel_x.max(0),
        panel_y.max(0),
        panel_w.max(0) as u32,
        panel_h.max(0) as u32,
        PANEL_BG,
    );

    // Banner.
    let banner_origin_x = panel_x + (panel_w - banner_text_w) / 2;
    let banner_origin_y = panel_y + PANEL_PAD_PX;
    draw_text(fb, ACQUIRING_TEXT, banner_origin_x, banner_origin_y, SCALE_PX, TEXT_FG);

    // Diagnostic rows, each centered.
    let mut row_y = banner_origin_y + banner_text_h + DIAG_LINE_GAP_PX;
    for row in &diag_rows {
        let n = row.chars().count() as i32;
        let row_w = n * diag_glyph_width + (n - 1).max(0) * diag_kern;
        let row_x = panel_x + (panel_w - row_w) / 2;
        draw_text(fb, row, row_x, row_y, DIAG_SCALE_PX, TEXT_FG);
        row_y += diag_row_h + DIAG_LINE_GAP_PX;
    }
}

fn draw_text(fb: &mut RenderFramebuffer, text: &str, x_origin: i32, y: i32, scale: i32, color: Color) {
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

/// 5×7 bit-pattern glyphs. Covers the characters used by
/// [`ACQUIRING_TEXT`] plus what the diagnostic counter rows need:
/// labels `B`/`S`/`F` and the decimal digits `0`–`9`. Any other
/// character is silently skipped by the renderer.
fn glyph(ch: char) -> Option<[&'static str; GLYPH_H as usize]> {
    match ch {
        'A' => Some([
            "01110", "10001", "10001", "11111", "10001", "10001", "10001",
        ]),
        'B' => Some([
            "11110", "10001", "10001", "11110", "10001", "10001", "11110",
        ]),
        'C' => Some([
            "01110", "10001", "10000", "10000", "10000", "10001", "01110",
        ]),
        'D' => Some([
            "11110", "10001", "10001", "10001", "10001", "10001", "11110",
        ]),
        'E' => Some([
            "11111", "10000", "10000", "11110", "10000", "10000", "11111",
        ]),
        'F' => Some([
            "11111", "10000", "10000", "11110", "10000", "10000", "10000",
        ]),
        'G' => Some([
            "01110", "10001", "10000", "10111", "10001", "10001", "01110",
        ]),
        'I' => Some([
            "01110", "00100", "00100", "00100", "00100", "00100", "01110",
        ]),
        'N' => Some([
            "10001", "11001", "10101", "10011", "10001", "10001", "10001",
        ]),
        'P' => Some([
            "11110", "10001", "10001", "11110", "10000", "10000", "10000",
        ]),
        'S' => Some([
            "01111", "10000", "10000", "01110", "00001", "00001", "11110",
        ]),
        'T' => Some([
            "11111", "00100", "00100", "00100", "00100", "00100", "00100",
        ]),
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
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use render_core::raster::Framebuffer as RasterFramebuffer;

    #[test]
    fn overlay_writes_white_pixels_inside_a_centered_black_panel() {
        let mut fb: RenderFramebuffer = RasterFramebuffer::new(800, 800);
        // Pre-fill so we can detect what the overlay actually changed.
        fb.clear(Color::new(0x10, 0x20, 0x30));
        render_acquiring_gps_overlay(&mut fb, None);

        // Sample the very center of the panel — that's between glyphs
        // (or inside one), but always inside the black panel. The
        // pre-fill would be (16, 32, 48); the overlay must have
        // overwritten with either black (panel) or white (text), not
        // left the underlying color visible.
        let pixels = fb.pixels();
        let cx = (fb.width() / 2) as usize;
        let cy = (fb.height() / 2) as usize;
        let idx = (cy * fb.width() as usize + cx) * 4;
        let r = pixels[idx];
        let g = pixels[idx + 1];
        let b = pixels[idx + 2];
        let is_panel_or_text = (r == 0 && g == 0 && b == 0)
            || (r == 0xFF && g == 0xFF && b == 0xFF);
        assert!(
            is_panel_or_text,
            "center pixel must be panel-black or text-white; got ({r},{g},{b})"
        );
    }

    #[test]
    fn overlay_text_includes_g_e_t_i_n_p_s() {
        // The glyph table has to cover the whole string; if a character
        // is missing the renderer silently skips it and the banner
        // looks wrong on device. This test fails fast if the table is
        // ever pruned without updating ACQUIRING_TEXT.
        for ch in ACQUIRING_TEXT.chars().filter(|c| *c != ' ') {
            assert!(
                glyph(ch).is_some(),
                "no glyph defined for '{ch}' but ACQUIRING_TEXT contains it"
            );
        }
    }

    #[test]
    fn diagnostic_rows_render_all_required_glyphs() {
        // `B/S/F` labels and every decimal digit must be in the
        // glyph table so the counter rows render without holes.
        for ch in ['B', 'S', 'F'] {
            assert!(glyph(ch).is_some(), "missing diagnostic label glyph {ch}");
        }
        for ch in '0'..='9' {
            assert!(glyph(ch).is_some(), "missing digit glyph {ch}");
        }
    }

    #[test]
    fn overlay_with_diagnostics_writes_text_pixels_below_the_banner() {
        // With `Some(diagnostics)` the panel grows downward to host
        // counter rows; the area directly below the banner must
        // contain at least one text-white pixel (a digit/letter
        // stroke) overwriting the pre-fill.
        let mut fb: RenderFramebuffer = RasterFramebuffer::new(800, 800);
        let prefill = Color::new(0x10, 0x20, 0x30);
        fb.clear(prefill);
        render_acquiring_gps_overlay(
            &mut fb,
            Some(GpsDiagnostics {
                bytes_seen: 1234,
                sentences_seen: 56,
                fixes_seen: 0,
                last_fix_age_ms: None,
            }),
        );

        // Walk the area below screen-center and expect at least one
        // white pixel from the counter-row strokes.
        let pixels = fb.pixels();
        let w = fb.width() as usize;
        let h = fb.height() as usize;
        let mut found_white = false;
        for y in (h / 2 + 80)..(h / 2 + 200).min(h) {
            for x in (w / 4)..(w * 3 / 4) {
                let idx = (y * w + x) * 4;
                if pixels[idx] == 0xFF && pixels[idx + 1] == 0xFF && pixels[idx + 2] == 0xFF {
                    found_white = true;
                    break;
                }
            }
            if found_white {
                break;
            }
        }
        assert!(
            found_white,
            "expected counter-row text pixels below the banner; found none"
        );
    }

    #[test]
    fn diag_row_renders_label_and_six_hex_digits() {
        assert_eq!(format_diag_row("BITS", 0), "BITS 0x000000");
        assert_eq!(format_diag_row("PING", 0x42), "PING 0x000042");
        assert_eq!(format_diag_row("PINS", 0xFF_FFFF), "PINS 0xFFFFFF");
        // Beyond 0xFF_FFFF we saturate so the row stays the same width.
        assert_eq!(format_diag_row("PINS", 0x1234_5678), "PINS 0xFFFFFF");
    }
}
