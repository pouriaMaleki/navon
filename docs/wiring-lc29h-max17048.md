# LC29H GPS + MAX17048 fuel gauge + HW-465A UPS — wiring reference

Hardware bring-up notes for the Waveshare ESP32-P4-WIFI6-Touch-LCD-3.4C
with the Quectel LC29H GNSS module, the Adafruit 5580 MAX17048 fuel
gauge, and the HW-465A 5V UPS module. Header connections are listed by
**silkscreen name with the verified J8 physical pin** — trust the
silkscreen if they ever disagree.

## Reserved GPIOs on the 3.4C

Do not repurpose any of these for the GPS, gauge, or power — they are
already claimed by the P4 core and its peripherals (confirmed against
the official Waveshare BSP and the board schematic, and by the
boot-time GPIO log — see `device/firmware/src/gps_uart.rs`):

| GPIO | Owner |
|---|---|
| 7, 8 | Touch (GT911) / panel CH422G I²C (SDA, SCL) — *shared*, see below |
| 9–13 | Audio I²S (ES8311/ES7210) + MCLK/SCLK/LRCK |
| 14–19 | Hosted-BLE SDIO link to the on-board ESP32-C6 |
| 23 | GT911 touch reset |
| 24, 25 | USB-Serial-JTAG console |
| 26, 27 | Panel backlight + JD9365 reset |
| 39–44 | microSD slot |
| 45–49 | SPI flash |
| 50, 51 | USB OTG D+/D− |
| 53 | Speaker PA enable |
| 54 | ESP32-C6 reset |

## Verified J8 40-pin header map

Extracted from the official Waveshare schematic
(`ESP32-P4-WIFI6-Touch-LCD-XC-Schematic.pdf`). Only the pins we care
about are listed; connect by **silkscreen name** — the pin numbers
below are the physical 2.54 mm header positions.

| J8 pin | Net |
|---|---|
| 1, 15, 31 | GND (confirmed) |
| 2 | GPIO30 (GPS RX) |
| 33 | GPIO29 (GPS TX) |
| 35 | ESP_I2C_SCL (GPIO8) |
| 37 | ESP_I2C_SDA (GPIO7) |
| 39 | 3V3 |
| 38 / 40 | VCC_5V (5V) |

Other pins on the header (schematic-verified): 3=GPIO47, 4=GPIO46,
5=GPIO52, 6=GPIO31, 7=GPIO48, 9=GPIO32, 10=GPIO34, 11=GPIO51,
13=USB1P1_N, 14=USB1P1_P, 16=GPIO49, 17=GPIO50, 18=GPIO36, 19=GPIO2,
20=GPIO35, 21=GPIO3, 24=GPIO4, 25=GPIO28, 26=GPIO5, 27=GPIO20,
29=GPIO21, 30=GPIO22, 32=GPIO38, 34=GPIO37. Note these are **ESP32
GPIO numbers, not header pin numbers** — "GPIO39–44 = microSD" has
nothing to do with header pins 39–44.

## GPS — LC29H (LC29HAAMD carrier board, pins V/G/T/R/P + SMA)

Same UART1 pins the firmware already uses for the NEO-6M. The LC29H
talks at its factory **115200 8N1** (NEO-6M was 9600); the firmware
constructs the port with `UartGpsSerial::new_lc29h_uart1()`.

| LC29H board | 3.4C header (J8 pin) | Function |
|---|---|---|
| V | 5V (38/40) or 3V3 (39) | carrier V pin accepts 3.3–5 V (~35 mA acquisition); either rail works |
| G | GND (1/15/31) | |
| T (module TXD) | GPIO30 (pin 2) | MCU RX ← module TX |
| R (module RXD) | GPIO29 (pin 33) | MCU TX → module RX (firmware never transmits; only binds the matrix route) |
| P (1PPS) | unconnected | optional: any free GPIO (e.g. GPIO31) if 1PPS is ever wanted |
| SMA | included L1/L5 active antenna | antenna DC bias comes from VDD_RF through the carrier board |

Antenna check: the antenna is **active**. If PINS stays 0 while PING
counts up, meter the SMA center pin with the module powered — it
should read ~3.3 V. Zero volts means the carrier board isn't feeding
VDD_RF; add an external bias-T on the antenna's DC input.

## Battery — MAX17048 tapped in parallel with the cell

The gauge only senses voltage — it is **not** a current sensor, so it
does not need to sit in series. The UPS is a possible ~15 W load
(~4 A at 3.7 V when boosting), so run the battery ↔ HW-465A wiring
with adequate gauge and tap the MAX17048 onto the same battery
terminals:

```
1S LiPo battery + ────► HW-465A B+        (heavy wire)
1S LiPo battery − ────► HW-465A B−        (heavy wire)
1S LiPo + ────────────► MAX17048 JST (either port)
1S LiPo − ────────────► MAX17048 JST GND
MAX17048 STEMMA QT ──► 3.4C header: GND, 3V3, SDA=GPIO7 (37), SCL=GPIO8 (35)
```

The two JST ports on the Adafruit board are wired in parallel
(pass-through), so battery-through-port-A and port-B-to-load also
works for small loads — but not for a 15 W UPS.

**Power the gauge's VIN from 3V3**, not 5V: the breakout's I²C
pull-ups are tied to VIN, and 5 V there would drive the P4's 3.3 V
I²C lines out of spec. QT cable color order (Adafruit standard):
black=GND, red=3V3, blue=SDA, yellow=SCL. The gauge's 0x36 address
does not collide with the GT911 (0x14/0x5D) or CH422G on the same
bus, and the board's external pull-ups make extra resistors
unnecessary.

Note: on the Adafruit breakout **VIN does not power the MAX17048
chip itself — the battery does** (the chip's VDD rail is jumpered to
the cell, since the chip's supply range is 2.5–4.5 V). A cell below
~2.5 V can therefore fail to appear on I²C at all: `no ACK at 0x36`
means either miswiring **or** a battery too dead to power the chip.
The corner readout appearing at all already proves the cell is above
~2.5 V.

Caveat: the 18650 inside the HW-465A's own holder cannot be measured
by the gauge (it's inside the UPS charge/discharge loop) — use the
external LiPo topology above.

## Power — HW-465A UPS

| HW-465A | To |
|---|---|
| B+/B− | LiPo directly (gauge tapped in parallel, above) |
| USB-C in | any 5 V charger, ≥2 A (charge + load simultaneously) |
| UPS +/− out | 3.4C **5V pin (38/40) + GND** on the 40-pin header (or a USB-C cable into the board's "USB 2.0 FS" Type-C port) |

**Before connecting anything: meter the UPS output and confirm ~5.0 V.**
The HW-465A (5 V), HW-465B (9 V) and HW-465C (12 V) modules look
essentially identical, and 9/12 V into `VCC_5V` would destroy the
board.

With USB-C input present the module passes 5 V through while charging;
on power loss it switches to battery boost without delay. When
flashing/debugging, powering the board via its own USB-C port at the
same time as the 5V header pin is generally fine (common 5 V rail) but
keep one source during bring-up to avoid confusion.

## Firmware surface

- `device/firmware/src/gps_uart.rs` — `LC29H_DEFAULT_BAUD` +
  `new_lc29h_uart1()`; reserved-GPIO table.
- `device/firmware/src/fuel_gauge.rs` — MAX17048 driver (VCELL 0x02 →
  mV, SOC 0x04 → %), shared legacy I²C master on GPIO7/8.
- `device/firmware/src/battery_overlay.rs` — persistent corner readout
  (`82% 3.92V`), hidden while the gauge is absent.
- `device/firmware/src/board_config.rs` — `FuelGaugeConfig`
  (SDA 7 / SCL 8 / addr 0x36 / poll 2 s).
