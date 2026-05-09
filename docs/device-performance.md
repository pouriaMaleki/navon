# Device performance — ESP32-P4 render pipeline

Reference for understanding, measuring, and improving per-frame performance
of the firmware running on the Waveshare ESP32-P4-WIFI6-Touch-LCD-3.4C
(800×800 round MIPI-DSI panel, JD9365 driver IC).

## Per-frame profile

The device boot loop logs one line per second with averaged per-phase
timings:

```
frame=N fps=X.X avg_work_ms=NN geometry=NN | query=Nms render=Nms convert=Nms push=Nms
```

| phase | what it is | scales with | typical cost (after current optimizations) |
|---|---|---|---|
| `query` | `MapSource::query` — spatial-grid lookup, layer/bounds filtering, allocating the geometry list | visible segment count | <1 ms idle, ~50 ms at 12k+ segs |
| `render` | `render_core::render_frame` — clear + Bresenham/stamp_circle for every visible polyline + overlay drawing | visible segment count, polyline thickness | ~45 ms idle, ~330 ms at 12k segs |
| `convert` | RGBA→RGB565 conversion or panel-fb memcpy on host; **0 on device** since render-core writes RGB565 directly into a buffer aliased by the panel backend | viewport size | 0 ms on device; ~5 ms on host (no-op aside from a pointer stash) |
| `push` | `esp_lcd_panel_draw_bitmap` — DMA from PSRAM into the DPI panel's auto-refresh framebuffer | viewport size, panel format | ~34 ms (constant; 1.28 MB at PSRAM bandwidth) |

The gap between `query+render+convert+push` and `avg_work_ms` is input
handling, runtime ECS step, and route-sync bookkeeping (typically
~30 ms total).

## Known floors

These are the lower bounds on this hardware **without** moving to a
different architecture. They are not currently movable by tuning the
existing code.

- **`push` ≈ 34 ms** — `esp_lcd_panel_draw_bitmap` memcpy into the DPI
  framebuffer. The DPI hardware reads its framebuffer continuously
  (~73 MB/s for 60 Hz × 800×800 × 2 bytes), and our memcpy competes
  for the same PSRAM bus. Eliminating this requires either Plan B
  (render directly into the DPI fb with `num_fbs: 2` swap) or PPA
  hardware blit.
- **`render` ≥ ~5 µs/segment minimum** — the rasterizer's per-pixel
  cost on a 360 MHz scalar RISC-V without SIMD is bounded by PSRAM
  random-access latency for AA edge writes. At 800×800 with thick
  AA strokes this caps at 200+ kpx/s effective fill rate.
- **PSRAM realistic bandwidth ~150 MB/s** — 200 MHz hex DDR theoretical
  ~400 MB/s, but real workloads (read + write streams competing with
  DPI refresh) see ~150 MB/s. **The boot-time `psram probe` log line
  is bogus** — its `dst.copy_from_slice(&src)` gets DCE'd by the
  optimizer because nothing reads `dst`. Don't trust the number.

## Optimization knobs not yet pulled

Listed in roughly decreasing payoff/risk order. None have been measured
on this device — confidence levels reflect bandwidth math + analysis,
not measurement.

| change | expected gain | effort | risk | notes |
|---|---|---|---|---|
| **DPI direct framebuffer** (`num_fbs: 2`, render into back buffer, IDF auto-flip) | -34 ms `push` (idle → ~10 fps) | 1-2 days | low-med | Cache flush + double-buffer bookkeeping. Doubles PSRAM fb footprint to 2.56 MB. |
| **Thin-line fast path** in `render_core::raster::draw_line` (skip `stamp_circle` when thickness=1) | -20-30% on `render` | half day | low | Most road layers are 1-2 px; stamp_circle iterates a 3×3 box per Bresenham point. Visible diff: thin roads look slightly crisper or harsher. |
| **`set_pixel_overwrite` for solid layers** | -15-30% on `render` | half day + audit | low-med | Audit per-layer: route-on-basemap is fine (drawn last); roads crossing each other change z-order from "max color wins" to "last drawn wins". |
| **Span-based scanline rasterizer** (replace Bresenham + stamp_circle with span fills) | up to -3× on `render` | ~2 weeks | medium-high | Memset-style writes are the fastest possible per-pixel cost. Anti-aliasing harder. |
| **PPA-accelerated `convert` / `push`** | -34 ms `push` + free color conversion in DMA | ~2-3 weeks | medium | IDF's `esp_driver_ppa` exists but bindings need to be added to our build (`extra_components`). PPA rotation is discrete 0/90/180/270 only — useless for track-up rotation. |
| **Multi-core pipeline** (Core 0: input/runtime/query, Core 1: rasterize) | up to ~2× throughput when CPU-bound | ~1 week | medium | Both cores share PSRAM bus → likely ~1.5× actual. Sync via double-buffer + mutex. |
| **Pre-baked raster tiles in flash** + CPU bilinear-rotated compositor | up to 60 fps idle, ~20 fps panning with rotation | ~6-8 weeks | low (architecturally) | The "commercial nav device" architecture. Vector rasterizer reserved for overlays only. See device-performance history for the design that got close. |

## Fundamental constraint

The ESP32-P4 has no GPU. We are rasterizing 800×800 of vector geometry
on a 360 MHz scalar RISC-V with no vector ISA, against PSRAM with
~150 MB/s realistic bandwidth.

For comparison: phone GPUs have ~100–500 GFLOPs and 25 GB/s memory
bandwidth, with dedicated rasterization silicon. A typical workload
that runs at 60 fps on a phone is doing 1000× the compute and 100× the
memory bandwidth of what we have. Real-time vector rendering on this
class of hardware is genuinely hard.

**Realistic upper bound without architectural change:** ~20 fps panning
at typical zoom, ~10 fps at deep zoom-out. Reaching consistent 30+ fps
requires either pre-rasterized tiles (move the vector work to build
time) or hardware acceleration the chip doesn't fully provide.

## Methodology

### Adding a new phase probe

`firmware::app::App::step_frame` populates a `PhaseTimings` struct on
every frame. To time a new section:

1. Add a `Duration` field to `PhaseTimings` in
   [firmware/src/app.rs](../device/firmware/src/app.rs).
2. Wrap the section with `Instant::now()` / `.elapsed()`.
3. Aggregate it in the per-second log loop in
   [firmware/src/esp_idf.rs `run_device_main`](../device/firmware/src/esp_idf.rs).

Cost is two `Instant::now()` calls per frame per probe, well below
microsecond accuracy on the IDF clock — negligible vs the work being
measured.

### Bisecting a regression

The per-second log line is the first place to look. Compare against
the known-good numbers below.

If a phase moved unexpectedly:
- `query` regressed → check `MapSource::query` allocations (the
  hot-path fix uses a frame-counter dedup + `SmallVec` inline storage;
  any code path that re-introduces per-frame heap churn shows up here).
- `render` regressed → likely a change to `render_core::raster::*` or
  a new draw call added without a sub-pixel/visibility cull.
- `convert` regressed → on device, the panel-pixel alias broke (check
  `panel_pixels_alias` is being set in `present_from_render`); on
  host, the RGBA→RGB565 conversion path is doing more work.
- `push` regressed → DPI hardware contention, or the panel's fb
  allocation moved.

### Known-good baselines (commit `ae1357a`, 2026-04-26)

Measured on the Waveshare ESP32-P4-WIFI6-Touch-LCD-3.4C, JD9365 panel,
PSRAM at 200 MHz, with a fixed GPS fix at the embedded map's center.

| state | total | phases (query/render/convert/push) | fps |
|---|---|---|---|
| idle (101 segs) | 115 ms | 0 / 44 / 0 / 34 ms | 7.6 |
| typical pan (~500 segs) | 130 ms | 1 / 60 / 0 / 34 ms | 6.5 |
| zoomed-out (~2k segs) | 170 ms | 8 / 88 / 0 / 34 ms | 5.0 |
| deep zoom-out (~13k segs) | 461 ms | 53 / 332 / 0 / 34 ms | 2.1 |

Cumulative since the perf work began: idle 4.5 → 7.6 fps (+69%), 2k-seg
pan 2.5 → 5.0 fps (+100%).
