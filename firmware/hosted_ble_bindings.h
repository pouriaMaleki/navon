// Bindgen-only shim for the local `hosted_ble` component. esp-idf-sys
// resolves `package.metadata.esp-idf-sys.extra_components.bindings_header`
// relative to the consuming crate's manifest dir, so the header has to
// live somewhere under `firmware/`. The component itself ships its public
// header at `components/hosted_ble/include/hosted_ble.h` — we just point
// bindgen at it via the C include path that ESP-IDF's CMake build wires
// up automatically.
#pragma once
#include "hosted_ble.h"
