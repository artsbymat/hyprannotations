# hyprannotations

A fast, lightweight, native screen annotation overlay designed specifically for **Hyprland** (Wayland).

Draw, highlight, and write text directly on top of your desktop windows in real time with minimal resource usage and zero bloat.

- **100% Native Rust & Wayland**: Built on `wayland-client` and `wlr-layer-shell`.
- **Zero Heavyweight GUI Frameworks**: No Electron, Qt, GTK, or Chromium.
- **Low Latency & Minimal Footprint**: Pure 2D rasterization using `tiny-skia` on Wayland shared memory (SHM) buffers.
- **Multi-Monitor Aware**: Renders independent per-output overlays respecting display scaling and rotation.
- **Desktop Pass-Through Mode**: Toggle drawing on/off instantly with `Tab` without clearing your annotations.

---

## Installation

### Prerequisites

You need standard build tools, Rust (edition 2024), and Wayland development libraries:

- **Arch Linux**:
  ```sh
  sudo pacman -S base-devel wayland libxkbcommon grim
  ```
- **Ubuntu / Debian**:
  ```sh
  sudo apt-get install build-essential pkg-config libwayland-dev libxkbcommon-dev grim
  ```
- **Fedora**:
  ```sh
  sudo dnf install gcc pkgconf-pkg-config wayland-devel libxkbcommon-devel grim
  ```

### Build from Source

```sh
git clone https://github.com/artsbymat/hyprannotations.git
cd hyprannotations
cargo build --release
```

The compiled binary will be located at `target/release/hyprannotations`. You can install it to your `$PATH`:

```sh
sudo install -Dm755 target/release/hyprannotations /usr/local/bin/hyprannotations
```

---

## Hyprland Configuration

Bind `hyprannotations` to a shortcut in your `~/.config/hypr/hyprland.conf`:

```ini
# Toggle screen annotation overlay
bind = SUPER SHIFT, A, exec, hyprannotations
```

> [!NOTE]
> Run a single instance at a time. The overlay automatically requests keyboard/pointer focus while drawing and releases input back to the desktop when toggled with `Tab` or closed with `Escape`.

---

## Controls & Shortcuts

| Shortcut | Tool / Action | Description |
| :--- | :--- | :--- |
| `P` | **Pen** | Freehand drawing |
| `H` | **Highlighter** | Semi-transparent strokes |
| `E` | **Eraser** | Click on an annotation to erase it |
| `L` | **Line** | Straight lines |
| `A` | **Arrow** | Directional arrows |
| `R` | **Rectangle** | Draw rectangles (supports outline or fill) |
| `O` | **Ellipse** | Draw circles and ellipses (supports outline or fill) |
| `T` | **Text** | Click to place, `Shift+Enter` for multiline, `Enter` to commit |
| `Q` | **Laser Pointer** | Temporary glowing trail that fades out automatically |
| `V` | **Select** | Click any object to select, drag to reposition |
| `Delete` | **Delete** | Delete the currently selected annotation |
| `Ctrl + Z` | **Undo** | Undo the last action on the current monitor |
| `Ctrl + Shift + Z` | **Redo** | Redo previously undone actions |
| `Ctrl + V` | **Paste Image** | Paste an image from the clipboard at the pointer position |
| `Ctrl + C` | **Copy Screenshot** | Copy the active monitor with annotations, excluding the toolbar |
| `Tab` | **Desktop Mode** | Pass pointer & keyboard input to desktop windows while keeping drawings visible |
| `Escape` | **Cancel / Exit** | Cancel active text input, or close the overlay |
| `Mouse Wheel` | **Stroke Width** | Scroll up/down to increase or decrease stroke size |
| `F1` | **Shortcut Help** | Show or hide the keyboard shortcut help overlay |

### Color Palette

- Click any of the **6 palette slots** in the toolbar to activate that color.
- Click the large active color indicator (or right-click a slot) to open the color picker.
- Palette selections and custom colors automatically persist in `~/.config/screen-annotation/palette.toml`.

---

## Configuration

`hyprannotations` works out of the box with sensible defaults, but can be customized via TOML.

Generate a default config file:

```sh
mkdir -p ~/.config/screen-annotation
hyprannotations --print-config > ~/.config/screen-annotation/config.toml
```

You can customize:
- `default_tool`: Initial tool on startup (`pen`, `highlighter`, etc.).
- `default_color`: Initial RGBA color.
- `stroke_width`: Default stroke size.
- `highlighter_opacity`: Opacity level (0–255).
- `font_size`: Default text size.
- `toolbar_position`: Screen coordinates `[x, y]` for the floating toolbar.
- `shortcuts`: Custom key bindings.

---

## CLI Options

```text
Usage: hyprannotations [OPTIONS]

Options:
      --print-config      Print default TOML configuration to stdout
      --preview <PNG>     Render a standalone toolbar preview PNG without Wayland
      --probe             Inspect compositor protocols without creating an overlay
      --smoke-test        Map overlay briefly, verify outputs and input release, then exit
  -h, --help              Print help
  -V, --version           Print version
```

---

## License

This project is licensed under the [MIT License](LICENSE).
