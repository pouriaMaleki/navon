//! Pairing-mode QR overlay rendering.
//!
//! While the device is unbonded, `App::step_frame` short-circuits the
//! map / runtime path and calls into this module to draw a QR code
//! containing the current `peripheral_address || ephemeral_secret`
//! payload. The companion scans the QR over the user's phone camera,
//! pulls out the secret, opens an encrypted-write connection (Bluedroid
//! Just Works pairing kicks in transparently), and writes the secret
//! back to the `pairing_confirm` characteristic — closing the OOB
//! confirmation handshake.
//!
//! `qrcodegen` is no_std + alloc-only so the same code compiles for the
//! host (test target) and the riscv32imafc-esp-espidf device target.

use qrcodegen::{QrCode, QrCodeEcc, Version};
use render_core::raster::Color;
use render_core::style::{COLOR_BACKGROUND_CANVAS, COLOR_TEXT_PRIMARY};

use crate::framebuffer::RenderFramebuffer;

/// Color used for the QR's dark modules. Kept high contrast against
/// `COLOR_BACKGROUND_CANVAS` so phone cameras pick the code up under
/// indoor lighting.
const QR_FG: Color = COLOR_TEXT_PRIMARY;
const QR_BG: Color = COLOR_BACKGROUND_CANVAS;

/// Pixel margin (quiet zone) the spec recommends around the code so the
/// camera's segmentation pass doesn't bleed surrounding UI into the
/// finder patterns.
const QUIET_MODULES: i32 = 4;

/// Encode the QR payload using ECC level Low (the payload fits the
/// available capacity comfortably and lower ECC keeps the QR's module
/// count down — easier on the camera at arms-length).
pub fn encode_payload(payload: &[u8]) -> Result<QrCode, qrcodegen::DataTooLong> {
    QrCode::encode_binary(payload, QrCodeEcc::Low)
}

/// Render `qr` into `fb`, centered. The integer module-pixel size is
/// chosen so the code fits inside the framebuffer bounds with the
/// quiet-zone margin baked in. Caller is responsible for clearing the
/// framebuffer first; this function only writes the QR area.
pub fn render_pairing_qr(qr: &QrCode, fb: &mut RenderFramebuffer) {
    let modules = qr.size() as i32 + 2 * QUIET_MODULES;
    let fb_w = fb.width() as i32;
    let fb_h = fb.height() as i32;
    let max_dim = fb_w.min(fb_h);
    if modules <= 0 || max_dim <= 0 {
        return;
    }
    let module_px = (max_dim / modules).max(1);
    let qr_px = module_px * qr.size() as i32;
    let origin_x = (fb_w - qr_px) / 2;
    let origin_y = (fb_h - qr_px) / 2;

    // Solid background panel under the QR (with quiet zone) so the
    // contrast stays clean on whatever the previous frame left behind.
    let panel_px = module_px * modules;
    let panel_x = (fb_w - panel_px) / 2;
    let panel_y = (fb_h - panel_px) / 2;
    fb.fill_rect_overwrite(
        panel_x,
        panel_y,
        panel_px as u32,
        panel_px as u32,
        QR_BG,
    );

    for module_y in 0..qr.size() {
        for module_x in 0..qr.size() {
            if !qr.get_module(module_x, module_y) {
                continue;
            }
            let x = origin_x + module_x * module_px;
            let y = origin_y + module_y * module_px;
            fb.fill_rect_overwrite(x, y, module_px as u32, module_px as u32, QR_FG);
        }
    }
}

/// Upper-bound check for the wire format: with version-6 ECC-L the QR
/// holds 134 bytes of binary data. The pairing payload is exactly 38
/// bytes (`peripheral_address(6) || ephemeral_secret(32)`), so v6 is
/// comfortable headroom and the camera will scan from arms-length.
pub fn fits_within_v6_ecc_low(qr: &QrCode) -> bool {
    qr.version().value() <= Version::new(6).value()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::{PERIPHERAL_ADDRESS_LEN, SECRET_LEN};
    use render_core::raster::Framebuffer as RasterFramebuffer;

    fn pairing_payload() -> Vec<u8> {
        let mut payload = Vec::with_capacity(PERIPHERAL_ADDRESS_LEN + SECRET_LEN);
        payload.extend_from_slice(&[0xAA; PERIPHERAL_ADDRESS_LEN]);
        payload.extend_from_slice(&[0x42; SECRET_LEN]);
        payload
    }

    #[test]
    fn qr_payload_encodes_within_v6_ecc_l_bound() {
        let qr = encode_payload(&pairing_payload()).expect("encode");
        assert!(
            fits_within_v6_ecc_low(&qr),
            "38-byte pairing payload should fit comfortably in v6 ECC-L; got version {}",
            qr.version().value(),
        );
    }

    #[test]
    fn render_pairing_qr_writes_finder_pattern_to_framebuffer() {
        let qr = encode_payload(&pairing_payload()).expect("encode");
        let mut fb: RenderFramebuffer = RasterFramebuffer::new(800, 800);
        // Pre-fill so we can detect that the renderer actually overwrote
        // the QR region (instead of leaving the default background).
        fb.clear(Color::new(0xFF, 0x00, 0xFF));
        render_pairing_qr(&qr, &mut fb);

        // Sample the very top-left module of the QR — for any version of
        // the code the (0,0) module is part of the top-left finder
        // pattern's outer ring, which is always dark.
        let modules = qr.size() as i32 + 2 * QUIET_MODULES;
        let fb_w = fb.width() as i32;
        let fb_h = fb.height() as i32;
        let max_dim = fb_w.min(fb_h);
        let module_px = max_dim / modules;
        let origin_x = (fb_w - module_px * qr.size() as i32) / 2;
        let origin_y = (fb_h - module_px * qr.size() as i32) / 2;
        // Sample the centre of the (0,0) module so we don't catch a
        // module-edge pixel on a sub-module rounding boundary.
        let sample_x = origin_x + module_px / 2;
        let sample_y = origin_y + module_px / 2;
        // Host build: framebuffer holds RGBA8888.
        let pixels = fb.pixels();
        let idx = ((sample_y as usize) * fb.width() as usize + sample_x as usize) * 4;
        // The QR foreground is COLOR_TEXT_PRIMARY = 0xFFFFFF; the
        // sampled module must be white, not the pink pre-fill.
        assert_eq!(pixels[idx], QR_FG.r, "R channel should be QR foreground");
        assert_eq!(pixels[idx + 1], QR_FG.g);
        assert_eq!(pixels[idx + 2], QR_FG.b);
    }
}
