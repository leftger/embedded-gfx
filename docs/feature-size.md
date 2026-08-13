# Feature-set flash size

`size_harness` is a tiny `no_std` binary that links `embedded-3dgfx` so we can
compare `.text` across feature recipes on `thumbv7em-none-eabihf`.

## Measure locally

```bash
./.github/scripts/measure_feature_size.sh
./.github/scripts/measure_feature_size.sh --check   # fail if minimal exceeds budget
```

Requires `arm-none-eabi-size` (or `llvm-size` / `rust-size`) and the
`thumbv7em-none-eabihf` target.

## Snapshot (2026-08-13)

| Variant | Features | `.text` bytes | Δ vs minimal |
| :--- | :--- | ---: | ---: |
| minimal | `row_width_320,depth-u16` | 62 896 | — |
| lighting | + `lighting` | 82 848 | +19 952 |
| full | + textured/raycast/scene/hud/painters/physics | 92 248 | +29 352 |

CI job **Minimal Feature Guard** runs the three library `cargo check` recipes
and `--check` on this harness (budget default **81 920** bytes / 80 KiB).

## Notes

- Release LTO already strips unused present-path code (`swapchain` /
  `display_backend` symbols are absent from the minimal ELF), so further
  feature-gating those modules is deferred unless a consumer forces them live.
- Real board flash is dominated by app code + buffers; measure firmware bins
  (e.g. `stm32wba-tftdisplay`) separately with `arm-none-eabi-size`.
