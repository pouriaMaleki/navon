// Bindgen-only shim for the espressif/esp_lcd_jd9365 managed component.
//
// `esp-idf-sys` resolves `package.metadata.esp-idf-sys.extra_components.
// bindings_header` relative to the consuming crate's manifest dir, so the
// header must live somewhere under `firmware/`. The real driver header
// ships inside the fetched managed component at
// `<build>/managed_components/espressif__esp_lcd_jd9365/include/esp_lcd_jd9365.h`
// — we just point bindgen at it via the C include path that ESP-IDF's
// CMake build wires up automatically.
#pragma once
#include "esp_lcd_jd9365.h"
