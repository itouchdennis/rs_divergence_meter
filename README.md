# nixie-wallpaper

Animated Nixie-tube wallpaper for Wayland using GTK4 + layer-shell. It renders a static background image and overlays animated digits, including a fixed dot position and a simple blur/glow on digit changes.

Inspired by the Steins;Gate divergence meter aesthetic. El Psy Congroo.

## Features
- Runs as a background layer on Wayland (no window decorations).
- PNG-based assets for background and digits.
- Configurable digit sequence and timing via constants in `src/main.rs`.
- Simple change blur by drawing the previous digit with small offsets.

## Assets
Create the assets folder and place PNGs under `src/assets`:
```bash
mkdir -p src/assets
```

Required filenames:
- `fond.png` (background)
- `0.png` ... `9.png` (digits)
- `dot.png` (separator)
- `empty.png` (blank tube)

This repo does not include the original Wallpaper Engine assets. If you use third-party assets, make sure you have permission or use your own originals. For reference, the inspiration source is:
https://steamcommunity.com/sharedfiles/filedetails/?id=1364278549&searchtext=steins%3Bgate

## Build
Prerequisites:
- Rust (stable)
- GTK4 development packages
- Wayland session

### Debian/Ubuntu
Install dependencies:
```bash
sudo apt update
sudo apt install -y build-essential pkg-config libgtk-4-dev
```
Install Rust (recommended via rustup):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Arch Linux
Install dependencies:
```bash
sudo pacman -S --needed base-devel pkgconf gtk4
```
Install Rust (if not installed):
```bash
sudo pacman -S --needed rustup
rustup default stable
```

Build:
```bash
cargo build
```

Run:
```bash
cargo run
```
## Autostart with niri
1) Build and install the binary:
```bash
cargo build --release
mkdir -p ~/.local/bin
cp target/release/nixie-wallpaper ~/.local/bin/
```
2) Add it to your niri config:
```ini
spawn-at-startup "sh" "-c" "sleep 1 && PATH_TO/nixie-wallpaper"
```
## Tuning
Edit constants in `src/main.rs` for scale, timing, and layout:
- `TUBE_SCALE`, `SPACING`
- `LEAD_RATE`, `TAIL_RATE`, `FIRST_GROUP_RATE`
- `TUBE_COUNT`, `DOT_IDX`, `FIRST_GROUP_END`

## Config (YAML)
You can override timing and layout via `~/.config/divergence_meter/config.yaml`.
If the file doesn't exist, it is created on first run. Changes are reloaded at runtime.
See `config.example.yaml` for a full example.
Image paths can be absolute or relative to the project root.
Alignment values:
- `tube_align_x`: `left`, `center`, `right`
- `tube_align_y`: `top`, `center`, `bottom`
- `bg_scale_mode`: `stretch`, `fit`, `fill`, `center`

Example:
```yaml
tube_count: 12
dot_idx: 1
first_group_end: 6
lead_rate: 30
spacing: 12.0
tube_scale: 1.0
step_ms: 50
hold_ms: 5000
step_interval_range: 3
empty_indices: [6, 7]
empty_chance_mod: 20
background_path: null
digit_paths: null
dot_path: null
empty_path: null
tube_offset_x: 0.0
tube_offset_y: 0.0
tube_align_x: center
tube_align_y: center
bg_scale_mode: stretch
```
