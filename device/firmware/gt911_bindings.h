// Bindgen-only shim for the espressif/esp_lcd_touch_gt911 managed component.
//
// Pulls in the GT911-specific factory function plus the base esp_lcd_touch
// API (`esp_lcd_touch_read_data`, `esp_lcd_touch_get_data`, the touch
// handle/data types) that the GT911 component depends on transitively.
#pragma once
#include "esp_lcd_touch.h"
#include "esp_lcd_touch_gt911.h"
