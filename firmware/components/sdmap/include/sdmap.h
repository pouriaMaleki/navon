// SD card map loader.
//
// Mounts a microSD card (FAT) read-only at /sdcard, reads /sdcard/map.svm
// into a heap_caps PSRAM buffer, then unmounts. Returns NULL on any error
// (no card, no file, OOM, mount failure) so the caller can fall back to
// the on-flash map partition.
//
// Safety: this code never writes, erases, or formats the card. The mount
// is configured with format_if_mount_failed=false, and the file is opened
// with mode "rb". Other files on the card are not enumerated or touched.

#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Load /sdcard/map.svm into a freshly-allocated PSRAM buffer.
/// On success, *out_len is set to the byte count and a non-NULL pointer
/// is returned. Caller must free it with sdmap_free().
/// On failure (no card, missing file, OOM, etc.), returns NULL.
uint8_t *sdmap_load(size_t *out_len);

void sdmap_free(uint8_t *buf);

#ifdef __cplusplus
}
#endif
