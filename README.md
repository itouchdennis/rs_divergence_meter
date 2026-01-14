# nixie-wallpaper

Animated Nixie-tube wallpaper for Wayland using GTK4 + layer-shell. It renders a static background image and overlays animated digits, including a fixed dot position and a simple blur/glow on digit changes.

Inspired by the Steins;Gate divergence meter aesthetic. El Psy Congroo.

## Features
- Runs as a background layer on Wayland (no window decorations).
- PNG-based assets for background and digits.
- Configurable digit sequence and timing via constants in `src/main.rs`.
- Simple change blur by drawing the previous digit with small offsets.

## Assets
Place PNGs under `src/assets`:
- `fond.png` (background)
- `0.png` ... `9.png` (digits)
- `dot.png` (separator)

## Build
Prerequisites:
- Rust (stable)
- GTK4 development packages
- Wayland session

Build:
```bash
cargo build
```

Run:
```bash
cargo run
```

## Tuning
Edit constants in `src/main.rs` for scale, timing, and layout:
- `TUBE_SCALE`, `SPACING`
- `LEAD_RATE`, `TAIL_RATE`, `FIRST_GROUP_RATE`
- `TUBE_COUNT`, `DOT_IDX`, `FIRST_GROUP_END`