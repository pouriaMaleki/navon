//! NEO-6M / NEO-7M / NEO-8M / Quectel LC29H GPS NMEA reader for
//! ESP32-P4 over UART.
//!
//! ## Reserved pins on the Waveshare ESP32-P4-WIFI6-Touch-LCD-3.4C
//!
//! Don't pick any of these for the GPS — they're already wired to other
//! peripherals on the board, confirmed against the boot-time GPIO log:
//!
//! | GPIO    | Owner                                           |
//! |---------|-------------------------------------------------|
//! | 7, 8    | Touch / panel I²C (SDA, SCL)                    |
//! | 14–19   | Hosted-BLE SDIO link to the on-board ESP32-C6   |
//! |         | (D0=14, D1=15, D2=16, D3=17, CLK=18, CMD=19)    |
//! | 23      | GT911 touch controller reset                    |
//! | 26, 27  | Panel backlight + JD9365 reset                  |
//! | 39–44   | microSD slot (also CSI camera pads)             |
//! | 54      | ESP32-C6 reset (co-processor reset; named `Slave_Reset` in upstream esp_hosted)    |
//!
//! The standard Raspberry-Pi UART convention (header pin 8 = GPIO14 = TX,
//! pin 10 = GPIO15 = RX) **does not work on this board**: GPIO14/15 are
//! D0/D1 of the C6 SDIO bus, and binding them to UART1 brings BLE down
//! within seconds (`sdmmc_send_cmd returned 0x107`).
//!
//! ## Default pinout
//!
//! UART0 is reserved for the USB-Serial-JTAG console / `EspLogger`, so
//! the GPS lives on UART1 at **GPIO29 (MCU TX → module RX) / GPIO30
//! (MCU RX ← module TX)** — the pair the Waveshare 3.4C's Pi-style
//! header exposes on its silkscreen, confirmed empirically by the
//! in-firmware RX-line scanner (see the history notes on the constants
//! below). Both are general-purpose pins the boot-time log shows
//! untouched.
//!
//! NEO-6M defaults: 9600 baud, 8N1, no flow control, emits standard
//! NMEA-0183 (`$GPRMC`, `$GPGGA`, `$GPGSV`, …) at 1 Hz.
//!
//! LC29H defaults: 115200 baud, 8N1, no flow control, same standard
//! NMEA-0183 sentence set with `GN` talker IDs (`$GNRMC`, `$GNGGA`, …)
//! at 1 Hz. The parser keys off the `RMC` suffix only, so the talker
//! change is transparent.
//!
//! Wire the modules like this:
//!
//! | NEO-6M | LC29H board (V/G/T/R) | ESP32-P4 | function          |
//! |--------|-----------------------|----------|-------------------|
//! | VCC    | V                     | 3.3 V    | power             |
//! | GND    | G                     | GND      | ground            |
//! | RX     | R (module RXD)        | GPIO29   | MCU TX → mod RX   |
//! | TX     | T (module TXD)        | GPIO30   | MCU RX ← mod TX   |
//!
//! The LC29H's V pin accepts 3.3–5 V; feeding it 3.3 V keeps the UART
//! logic levels safely inside the P4's 3.3 V domain. The `P` (1PPS) pin
//! stays unconnected — the runtime decodes fixes from NMEA alone.
//!
//! The MCU never transmits to the module (the runtime doesn't push NMEA
//! configuration commands), so the TX wire can be left disconnected if
//! GPIO29 isn't broken out — RX-only is enough for fix decoding.
//!
//! ## Customizing
//!
//! If GPIO29/30 aren't accessible on the breakout you're using, call
//! [`UartGpsSerial::new`] directly with whatever pair *is* exposed —
//! ESP32-P4's GPIO matrix can route UART1 to any GPIO. Stay clear of the
//! reserved list above.
//!
//! ## Implementation notes
//!
//! `read_sentence` is non-blocking: it tops up an internal byte buffer
//! from the UART RX FIFO with `ticks_to_wait = 0`, then drains it until
//! the first `\n`. Callers (the runtime frame loop) poll once per
//! 16 ms tick — far faster than NEO-6M's 1 Hz default sentence rate, so
//! sentences come back at most one frame after they finish on the wire.
//!
//! NEO-6M power note: most "GY-NEO6MV2" breakout boards include an MIC5205
//! LDO that needs ≥ 3.6 V on its input to regulate properly. Powering
//! VCC from a 3.3 V rail usually still works because the LDO drops out
//! and passes ~3.3 V straight through, but if the module fails to
//! acquire satellites or repeatedly browns out, move VCC to a 5 V rail
//! — the module's RX/TX lines stay 3.3 V-tolerant either way.
//!
//! LC29H antenna note: the included L1/L5 antenna is *active* — it
//! draws its DC bias from the module's VDD_RF pin through the carrier
//! board's SMA connector. If the module never locks (PINS stays 0 with
//! PING counting up), meter the SMA center pin: it should read ~3.3 V
//! with the module powered. Zero volts means the carrier board isn't
//! feeding the antenna, and the antenna needs an external bias-T on
//! its own DC input.

#![cfg(target_os = "espidf")]

use std::collections::VecDeque;

use esp_idf_svc::sys::{
    self, esp_err_t, gpio_config, gpio_config_t, gpio_int_type_t_GPIO_INTR_DISABLE,
    gpio_mode_t_GPIO_MODE_INPUT, gpio_get_level, gpio_pullup_en,
    soc_periph_uart_clk_src_legacy_t_UART_SCLK_DEFAULT, uart_config_t,
    uart_driver_install, uart_hw_flowcontrol_t_UART_HW_FLOWCTRL_DISABLE,
    uart_parity_t_UART_PARITY_DISABLE, uart_param_config, uart_port_t,
    uart_port_t_UART_NUM_1, uart_read_bytes, uart_set_pin,
    uart_stop_bits_t_UART_STOP_BITS_1, uart_word_length_t_UART_DATA_8_BITS, UART_PIN_NO_CHANGE,
};

use crate::esp_idf::{EspIdfError, EspIdfGpsSerial};

/// Default NEO-6M serial bit-rate. Configurable via NMEA `PUBX,41` if you
/// reflash the module, but every NEO-6M I've seen ships at 9600.
pub const NEO6M_DEFAULT_BAUD: u32 = 9600;

/// Default Quectel LC29H serial bit-rate. Configurable via
/// `$PAIR864,0,0,<baud>*<checksum>`, but the module ships at 115200
/// (8N1, no flow control).
pub const LC29H_DEFAULT_BAUD: u32 = 115_200;

/// MCU TX → module RX. We don't actually drive this — the runtime never
/// sends NMEA configuration commands — but `uart_set_pin` requires a TX
/// pin to bind the matrix routing. GPIO29 is free on this SKU; if the
/// operator's GPS-RX wire is on a different physical pin than expected,
/// that's harmless because the firmware never transmits.
///
/// Physical locations on the 3.4C's 40-pin J8 header (verified against
/// the Waveshare schematic): GPIO29 = **J8 pin 33**, GPIO30 = **J8 pin
/// 2**. An earlier revision of this comment claimed the two pins were
/// adjacent on the header — they are not.
///
/// History: GPIO14 collided with D0 of the on-board ESP32-C6 SDIO bus
/// and broke hosted BLE; GPIO20 was a general-purpose pin but not the
/// one Waveshare's silkscreen labels expose to the GPIO header on this
/// SKU.
pub const WAVESHARE_3P4C_GPS_TX_GPIO: i32 = 29;

/// MCU RX ← module TX. **GPIO30** on this SKU — confirmed empirically
/// by the in-firmware RX-line scanner, which detected real UART
/// traffic on this pin (≈28 transitions per 12 ms sampling window
/// with the internal pull-down active, exactly the signature of a
/// 9600-baud NMEA stream sampled near the Nyquist limit). Physical
/// location: **J8 pin 2** on the 40-pin header (schematic-verified).
///
/// History: GPIO15 collided with D1 of the C6 SDIO bus; GPIO21 was a
/// matrix-routable pin but not where the Waveshare 3.4C's Pi-style
/// header actually breaks out the UART RX wire.
pub const WAVESHARE_3P4C_GPS_RX_GPIO: i32 = 30;

/// Discard partial lines that exceed this size — a well-formed NMEA-0183
/// sentence is ≤ 82 bytes including CR/LF. Anything past that is line
/// noise (electrical, baud mismatch, half-duplex collision) and we'd
/// rather drop the run-on than feed garbage to the parser.
const MAX_SENTENCE_LEN: usize = 256;

/// Generous RX FIFO. NEO-6M emits ~70 bytes/s steady-state at 1 Hz; even
/// at the maximum 10 Hz output rate (~700 B/s) we drain on every 16 ms
/// frame, so 1 KiB has many seconds of slack if the loop ever stalls.
const UART_RX_BUF_SIZE: i32 = 1024;

/// Read at most this many bytes per `read_sentence` call. One UART RX
/// FIFO refill per frame is plenty.
const UART_READ_CHUNK: usize = 256;

/// UART1-backed NMEA-0183 line reader. Owns the legacy UART driver
/// installation for its port; dropping the struct does not uninstall the
/// driver because the runtime keeps it alive for the entire boot.
pub struct UartGpsSerial {
    uart_num: uart_port_t,
    /// Bytes read from the FIFO that have not yet been delimited by `\n`.
    pending: VecDeque<u8>,
    /// Bytes accumulated for the *current* sentence so far.
    line: String,
    bytes_seen: u64,
    sentences_seen: u64,
}

impl std::fmt::Debug for UartGpsSerial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UartGpsSerial")
            .field("uart_num", &self.uart_num)
            .field("bytes_seen", &self.bytes_seen)
            .field("sentences_seen", &self.sentences_seen)
            .finish()
    }
}

impl UartGpsSerial {
    /// Bring up UART1 at the Waveshare 3.4C header pinout for a NEO-6M /
    /// NEO-7M / NEO-8M module at 9600 baud.
    pub fn new_neo6m_uart1() -> Result<Self, EspIdfError> {
        Self::new(
            uart_port_t_UART_NUM_1,
            WAVESHARE_3P4C_GPS_TX_GPIO,
            WAVESHARE_3P4C_GPS_RX_GPIO,
            NEO6M_DEFAULT_BAUD,
        )
    }

    /// Bring up UART1 at the Waveshare 3.4C header pinout for a Quectel
    /// LC29H (LC29HAAMD carrier board) at its factory 115200 baud. Same
    /// GPIO29/30 wiring as the NEO-6M — the LC29H just talks faster.
    pub fn new_lc29h_uart1() -> Result<Self, EspIdfError> {
        Self::new(
            uart_port_t_UART_NUM_1,
            WAVESHARE_3P4C_GPS_TX_GPIO,
            WAVESHARE_3P4C_GPS_RX_GPIO,
            LC29H_DEFAULT_BAUD,
        )
    }

    /// Generic constructor — installs the UART driver, applies 8N1 with
    /// the requested baud, and routes TX/RX to the given GPIOs.
    ///
    /// Calling this twice for the same `uart_num` will fail at
    /// `uart_driver_install` with `ESP_ERR_INVALID_STATE`; that's fine
    /// because the runtime brings GPS up exactly once during boot.
    pub fn new(
        uart_num: uart_port_t,
        tx_gpio: i32,
        rx_gpio: i32,
        baud: u32,
    ) -> Result<Self, EspIdfError> {
        // Probe the RX line *before* the UART driver takes the pin
        // over, so we can tell the operator whether the wire is even
        // electrically alive. Configure the pin as a plain digital
        // input with internal pull-up, then sample the level a few
        // times. The three observable states diagnose the three
        // failure modes that can leave `bytes_seen` stuck at 0:
        //
        // * `idle_high=N, idle_low=0` (steady high) → wire is good and
        //   either UART is idle (module silent) or the module's TX
        //   driver is parked at 3.3 V. Most likely failure here is
        //   "module is alive but not transmitting" (no power, dead, or
        //   wrong baud — pull-up alone can't tell).
        // * `idle_low=N, idle_high=0` → line shorted to GND or driven
        //   low by something. Check ground loops / wrong wiring.
        // * `idle_high≈idle_low` (toggling) → real activity on the
        //   line! Either bytes are arriving (good) or the baud is
        //   wrong and we're sampling mid-byte (still good news — the
        //   wire works). If `bytes_seen` is still 0 after this, the
        //   problem is baud-rate, not wiring.
        // * `idle_high=0, idle_low=0` → `gpio_get_level` returned no
        //   samples; only happens if the pin number is invalid.
        probe_rx_line(rx_gpio);

        let mut config = uart_config_t::default();
        config.baud_rate = baud as core::ffi::c_int;
        config.data_bits = uart_word_length_t_UART_DATA_8_BITS;
        config.parity = uart_parity_t_UART_PARITY_DISABLE;
        config.stop_bits = uart_stop_bits_t_UART_STOP_BITS_1;
        config.flow_ctrl = uart_hw_flowcontrol_t_UART_HW_FLOWCTRL_DISABLE;
        config.rx_flow_ctrl_thresh = 0;
        // The bindings expose the `source_clk` field through an
        // anonymous union (LP-UART variants share the slot). Writing
        // a union field is safe in stable Rust; we never read the
        // alternate variant.
        config.__bindgen_anon_1.source_clk = soc_periph_uart_clk_src_legacy_t_UART_SCLK_DEFAULT;

        // Install first so the UART hardware is ready before we touch
        // its registers via `uart_param_config` / `uart_set_pin`.
        // tx_buffer_size = 0: RX-only path. queue_size = 0: no event queue.
        check(
            unsafe {
                uart_driver_install(
                    uart_num,
                    UART_RX_BUF_SIZE,
                    0,
                    0,
                    core::ptr::null_mut(),
                    0,
                )
            },
            "uart_driver_install",
        )?;
        check(
            unsafe { uart_param_config(uart_num, &config) },
            "uart_param_config",
        )?;
        check(
            unsafe {
                uart_set_pin(uart_num, tx_gpio, rx_gpio, UART_PIN_NO_CHANGE, UART_PIN_NO_CHANGE)
            },
            "uart_set_pin",
        )?;
        // Re-enable the internal pull-up on RX *after* uart_set_pin
        // (which resets pin pull state). With pull-up on, a
        // disconnected RX wire idles high — same level as a live UART
        // idle line. That keeps the FIFO from being clogged with
        // spurious framing errors when the module is not yet wired,
        // but doesn't change behavior when the wire is good (the
        // module's TX driver overrides the weak ~45 kΩ pull-up).
        let _ = unsafe { gpio_pullup_en(rx_gpio) };

        log::info!(
            "gps uart: UART{} up @ {} baud 8N1, tx_gpio={} rx_gpio={}",
            uart_num,
            baud,
            tx_gpio,
            rx_gpio,
        );

        Ok(Self {
            uart_num,
            pending: VecDeque::with_capacity(UART_READ_CHUNK),
            line: String::with_capacity(MAX_SENTENCE_LEN),
            bytes_seen: 0,
            sentences_seen: 0,
        })
    }

    pub fn bytes_seen(&self) -> u64 {
        self.bytes_seen
    }

    pub fn sentences_seen(&self) -> u64 {
        self.sentences_seen
    }
}

impl EspIdfGpsSerial for UartGpsSerial {
    fn bytes_seen(&self) -> u64 {
        self.bytes_seen
    }

    fn read_sentence(&mut self) -> Result<Option<String>, EspIdfError> {
        // Top up the pending byte buffer with whatever the UART driver
        // has buffered right now. ticks_to_wait = 0 → non-blocking; if
        // no bytes are queued we fall through and try to drain whatever
        // we already had from a previous call.
        let mut chunk = [0_u8; UART_READ_CHUNK];
        let read = unsafe {
            uart_read_bytes(
                self.uart_num,
                chunk.as_mut_ptr() as *mut core::ffi::c_void,
                chunk.len() as u32,
                0,
            )
        };
        if read < 0 {
            return Err(EspIdfError::Io(format!(
                "uart_read_bytes failed: {read}"
            )));
        }
        if read > 0 {
            self.bytes_seen += read as u64;
            self.pending.extend(chunk[..read as usize].iter().copied());
        }

        while let Some(byte) = self.pending.pop_front() {
            if byte == b'\n' {
                let sentence = std::mem::take(&mut self.line);
                self.line.reserve(MAX_SENTENCE_LEN);
                self.sentences_seen += 1;
                return Ok(Some(sentence));
            }
            if byte == b'\r' {
                continue;
            }
            if self.line.len() >= MAX_SENTENCE_LEN {
                // Run-on garbage — drop the partial line and resync at
                // the next newline.
                self.line.clear();
                continue;
            }
            self.line.push(byte as char);
        }

        Ok(None)
    }
}

fn check(status: esp_err_t, op: &str) -> Result<(), EspIdfError> {
    if status == sys::ESP_OK as esp_err_t {
        Ok(())
    } else {
        Err(EspIdfError::Io(format!("{op} failed: {status}")))
    }
}

/// Probe the electrical state of the RX line *before* the UART driver
/// takes the pin over. Runs two samples back-to-back: once with an
/// internal pull-down active, once with an internal pull-up active.
/// The pair of results disambiguates the four physical failure modes
/// that can leave `bytes_seen` stuck at 0:
///
/// * pull-down sees HIGH **and** pull-up sees HIGH ⇒ wire is connected
///   to an active 3.3 V driver (module's TX in idle). If
///   `transitions == 0` on both, the module is alive but silent —
///   suspect dead module or wrong baud-rate.
/// * pull-down sees LOW **but** pull-up sees HIGH ⇒ line is **floating**.
///   Nothing is driving GPIO{rx_gpio}; either the wire isn't actually
///   attached, the GPS module's TX wire is on the wrong pin, the
///   wires are swapped (module's RX is connected here instead — RX is
///   an input, doesn't drive), or the module isn't powered.
/// * pull-down sees LOW **and** pull-up sees LOW ⇒ line is shorted
///   to GND. Check wiring for a short.
/// * either side shows transitions ⇒ real UART traffic. Wire is good;
///   if bytes still don't decode, the baud rate is wrong.
fn probe_rx_line(rx_gpio: i32) {
    let (pd_high, pd_low, pd_trans) = sample_rx(rx_gpio, /* pull_up = */ false);
    let (pu_high, pu_low, pu_trans) = sample_rx(rx_gpio, /* pull_up = */ true);

    let pd_verdict = classify(pd_high, pd_low, pd_trans);
    let pu_verdict = classify(pu_high, pu_low, pu_trans);

    let combined: &str = match (pd_verdict, pu_verdict) {
        (LineState::Toggling, _) | (_, LineState::Toggling) => {
            "TOGGLING — real UART activity on the wire. \
             If bytes_seen stays 0, suspect wrong baud rate \
             (NEO-6M ships at 9600, LC29H at 115200 — try the other one)."
        }
        (LineState::High, LineState::High) => {
            "WIRE GOOD, MODULE SILENT — line is being held high by an external 3.3 V driver \
             (module's TX in idle). Module is alive but not transmitting NMEA. \
             Check the power LED on the breakout, verify VCC reads ~3.3 V, or \
             consider that the module's baud is something other than 9600."
        }
        (LineState::Low, LineState::High) => {
            "FLOATING — no external connection on this pin (or GPS not installed). \
             Device will boot normally and show a GETTING GPS overlay until a fix arrives. \
             If GPS is wired: check TX/RX aren't swapped, verify VCC ~3.3 V, or call \
             scan_for_active_high_pins() manually to locate the actual pin."
        }
        (LineState::Low, LineState::Low) => {
            "SHORT TO GROUND — line is being pulled low even with the internal pull-up. \
             Check that you didn't tie the GPS TX wire to GND or a low-driven pin."
        }
        // Anything else is a sampling artifact; should be impossible
        // unless the GPIO matrix is misbehaving.
        _ => "INDETERMINATE — please rerun, or pick a different RX GPIO",
    };

    log::info!(
        "gps uart: RX line probe on GPIO{}: pull-down sample → high={} low={} transitions={}; \
         pull-up sample → high={} low={} transitions={}. Verdict: {}",
        rx_gpio,
        pd_high, pd_low, pd_trans,
        pu_high, pu_low, pu_trans,
        combined,
    );

}

/// Probe a hand-curated set of likely-free GPIOs and log any that
/// show an external 3.3 V driver (or activity). Skips the pins we
/// already know are owned by other peripherals on the Waveshare 3.4C
/// (touch I²C, SDIO to C6, panel reset/backlight, microSD, C6 reset).
/// Boot-time cost: ~25 pins × ~12 ms = ~300 ms total, only when the
/// configured RX pin came back FLOATING — i.e. only when this scan
/// is actually useful.
fn scan_for_active_high_pins(skip_gpio: i32) {
    // Conservative candidate list. Avoids:
    //   * 7, 8        (touch / panel I²C)
    //   * 14–19       (hosted-BLE SDIO bus)
    //   * 23          (GT911 touch reset)
    //   * 24, 25      (USB-Serial-JTAG on ESP32-P4 — driving these
    //                  externally during boot can break the console)
    //   * 26, 27      (panel backlight / JD9365 reset)
    //   * 39–44       (microSD slot / CSI camera pads)
    //   * 45–49       (SPI flash on most ESP32-P4 modules)
    //   * 50, 51      (USB OTG D+/D-)
    //   * 54          (ESP32-C6 reset)
    //
    // Anything left is a general-purpose pin we can read safely as
    // an input. The Waveshare 3.4C breakouts (SH1.0 connectors / Pi
    // header) draw from this set.
    const CANDIDATES: &[i32] = &[
        0, 1, 2, 3, 4, 5, 6, 9, 10, 11, 12, 13, 20, 21, 22, 28, 29, 30, 31, 32, 33, 34, 35, 36,
        37, 38, 52, 53,
    ];

    let mut active_high: Vec<i32> = Vec::new();
    let mut active_toggle: Vec<i32> = Vec::new();
    for &gpio in CANDIDATES {
        if gpio == skip_gpio {
            // Already reported above with full detail.
            continue;
        }
        let (high, low, trans) = scan_sample(gpio);
        // Pull-down enabled; "external 3.3 V driver" = high samples
        // with no lows, no transitions. Activity = many transitions.
        if trans > 3 {
            active_toggle.push(gpio);
            log::info!(
                "gps uart: scan GPIO{} → ACTIVITY (high={} low={} transitions={})",
                gpio, high, low, trans,
            );
        } else if low == 0 && high > 0 {
            active_high.push(gpio);
            log::info!(
                "gps uart: scan GPIO{} → external HIGH driver (high={} low={})",
                gpio, high, low,
            );
        }
    }

    if active_high.is_empty() && active_toggle.is_empty() {
        log::warn!(
            "gps uart: scan found NO externally-driven candidate GPIOs. \
             That means either (a) the GPS module is not powered (a powered \
             NEO-6M idles its TX line at 3.3 V), so no GPIO would see a high \
             driver — verify VCC ↔ GND reads ~3.3 V with a multimeter, or \
             (b) your GPS-TX wire is on a pin we deliberately skipped \
             (touch I²C / SDIO / SD / flash / USB pins) — move it to any \
             pin in this set and re-test: {:?}",
            CANDIDATES
        );
    } else {
        log::warn!(
            "gps uart: scan results — {} pin(s) look externally driven HIGH: {:?}, \
             {} pin(s) showed activity: {:?}. \
             Your GPS-TX wire is probably on one of these. Update \
             WAVESHARE_3P4C_GPS_RX_GPIO in firmware/src/gps_uart.rs to the \
             matching number and reflash.",
            active_high.len(), active_high,
            active_toggle.len(), active_toggle,
        );
    }
}

/// Quick variant of `sample_rx` for the scan: pull-down only, fewer
/// samples (~12 ms per pin). We only need to distinguish "externally
/// driven high" / "floating" / "transitioning"; we don't need the
/// per-pin precision the configured-pin probe uses.
fn scan_sample(gpio: i32) -> (u32, u32, u32) {
    let cfg = gpio_config_t {
        pin_bit_mask: 1_u64 << (gpio as u64),
        mode: gpio_mode_t_GPIO_MODE_INPUT,
        pull_up_en: sys::gpio_pullup_t_GPIO_PULLUP_DISABLE,
        pull_down_en: sys::gpio_pulldown_t_GPIO_PULLDOWN_ENABLE,
        intr_type: gpio_int_type_t_GPIO_INTR_DISABLE,
        ..Default::default()
    };
    unsafe { gpio_config(&cfg) };
    unsafe { sys::esp_rom_delay_us(1_000) };

    let mut high = 0_u32;
    let mut low = 0_u32;
    let mut transitions = 0_u32;
    let mut prev: Option<i32> = None;
    for _ in 0..60 {
        let level = unsafe { gpio_get_level(gpio) };
        if level == 0 {
            low += 1;
        } else {
            high += 1;
        }
        if let Some(p) = prev {
            if p != level {
                transitions += 1;
            }
        }
        prev = Some(level);
        unsafe { sys::esp_rom_delay_us(200) };
    }
    (high, low, transitions)
}

#[derive(Debug, Clone, Copy)]
enum LineState {
    High,
    Low,
    Toggling,
}

fn classify(high: u32, low: u32, transitions: u32) -> LineState {
    if transitions > 5 {
        LineState::Toggling
    } else if low == 0 && high > 0 {
        LineState::High
    } else if high == 0 && low > 0 {
        LineState::Low
    } else if transitions == 0 {
        // Pure majority decision when sampling went one-sided but
        // not 100%; covers tiny bit of noise without being fooled
        // into reporting "toggling".
        if high >= low { LineState::High } else { LineState::Low }
    } else {
        LineState::Toggling
    }
}

fn sample_rx(rx_gpio: i32, pull_up: bool) -> (u32, u32, u32) {
    let cfg = gpio_config_t {
        pin_bit_mask: 1_u64 << (rx_gpio as u64),
        mode: gpio_mode_t_GPIO_MODE_INPUT,
        pull_up_en: if pull_up {
            sys::gpio_pullup_t_GPIO_PULLUP_ENABLE
        } else {
            sys::gpio_pullup_t_GPIO_PULLUP_DISABLE
        },
        pull_down_en: if pull_up {
            sys::gpio_pulldown_t_GPIO_PULLDOWN_DISABLE
        } else {
            sys::gpio_pulldown_t_GPIO_PULLDOWN_ENABLE
        },
        intr_type: gpio_int_type_t_GPIO_INTR_DISABLE,
        ..Default::default()
    };
    unsafe { gpio_config(&cfg) };
    // The internal pull resistors take a moment to settle on a
    // floating line — without this gap the first dozen samples
    // routinely come back as the value the line was charged to during
    // the previous sample.
    unsafe { sys::esp_rom_delay_us(2_000) };

    // ~5 kHz × 50 ms = 250 samples per round.
    let mut high = 0_u32;
    let mut low = 0_u32;
    let mut transitions = 0_u32;
    let mut prev: Option<i32> = None;
    for _ in 0..250 {
        let level = unsafe { gpio_get_level(rx_gpio) };
        if level == 0 {
            low += 1;
        } else {
            high += 1;
        }
        if let Some(p) = prev {
            if p != level {
                transitions += 1;
            }
        }
        prev = Some(level);
        unsafe { sys::esp_rom_delay_us(200) };
    }
    (high, low, transitions)
}
