# GlideVK

**GlideVK** is a 3Dfx Glide 3.0 (`glide3x`) graphics wrapper written in Rust and backed by Vulkan. It translates legacy 3Dfx Voodoo Glide API calls into modern Vulkan 1.2+ rendering commands, producing a drop-in 32-bit `glide3x.dll` for classic Windows games (natively on Windows or via Wine on Linux and macOS/Apple silicon) alongside native shared libraries (`libglide3x.so` / `libglide3x.dylib`) for testing and development.

## Features

- **Glide 3.0 ABI Compatibility**
  - Exports the standard Glide 3.0 C/`stdcall` ABI (including Win32 `_name@N` symbol decorations and `grGetProcAddress` lookup).
  - Configurable dynamic vertex layouts (`grVertexLayout`) supporting screen/clip coordinates, `PARGB` packed or float colors, `Z`/`OOW` depth, and dual-TMU `STW` coordinates with perspective-correct `Q` division.
  - Dual Texture Mapping Units (`TMU0` / `TMU1`) with all 11 Glide color/alpha combine functions, 12 combine factors, inversion flags, constant colors, chromakey discard, alpha testing, scissor clipping (`grClipWindow`), Z/W depth buffering, and 64-entry Glide fog tables (`grFogTable` / `grFogMode`).
  - Palette and compressed texture decoding (`P8`, `AP88`, `P8` with 6-6-6-6 alpha palette extensions, `NCC` YIQ tables, `RGB332`, `ARGB8332`, `RGB565`, `ARGB1555`, `ARGB4444`, intensity/alpha formats).
  - Hardware gamma correction via `guGammaCorrectionRGB` and `grLoadGammaTable` with automatic sRGB swapchain compensation.

- **Linear Framebuffer (LFB) Support**
  - Dedicated logical front and back images decoupled from presentation scaling.
  - Supports `grLfbLock` / `grLfbUnlock`, `grLfbWriteRegion`, and `grLfbReadRegion` across packed 16-bit (`RGB565`, `RGB555`, `ARGB1555`) and 32-bit (`RGB888`, `ARGB8888`) formats while preserving untouched pixels on partial writes.

- **Batched Bindless Vulkan Renderer**
  - Uses Vulkan 1.2 descriptor indexing (`VK_EXT_descriptor_indexing`) to bind up to 2,046 textures per submission in a single descriptor array, with automatic draw-batch splitting and LRU texture cache eviction.
  - Packs combine modes, constant colors, and chromakey state per vertex and batches per-frame texture uploads into single command buffer submissions to minimize CPU/GPU synchronization overhead.

- **Display Scaling, Filtering & Window Controls**
  - Fullscreen or windowed display with fixed resolution presets (`640x480` up to `1920x1440`, `desktop`, or custom dimensions), optional resizable windows, and 4:3 pillarbox/letterbox aspect ratio preservation.
  - Three presentation upscale filters: sharp anti-aliased nearest-neighbor (`linear`), hardware `bilinear`, and 9-tap Catmull-Rom bicubic (`cubic`).
  - Win32 window subclassing that maps mouse coordinates through scaled/letterboxed viewports and confines the OS cursor to the active client area (`clip_cursor`).
  - Optional built-in 7-segment FPS overlay and `Ctrl+Shift+F12` hotkey on Windows/Wine to pause GPU rendering while keeping the game loop running at 25 Hz.

- **Wine & Cross-Platform Hardening**
  - Works around Wine 11's 32-bit WoW64 host-visible `vkFreeMemory` virtual address-space leak via `VK_EXT_external_memory_host` (`VirtualAlloc` host-pointer import).
  - Includes [`tools/enable_wine_dep.py`](tools/enable_wine_dep.py) and [macOS Wine + MoltenVK notes](WINE_MACOS.md) to eliminate Wine no-DEP memory-protection stalls on Apple silicon.

---

## Repository Structure

```text
glidevk/
├── Cargo.toml                 # Workspace root (glidevk + glide3x) and Wine-friendly dev profile
├── Makefile                   # Convenience targets for native and 32-bit Windows builds/tests
├── glidevk.toml.example       # Documented configuration template
├── glidevk/                   # Core Glide 3.0 state machine & Vulkan backend
│   ├── build.rs               # Compiles GLSL shaders into SPIR-V via glslc at build time
│   └── src/
│       ├── lib.rs             # Glide state tracking, LFB/texture APIs, FPS overlay
│       ├── api.rs             # String/parameter queries, proc-address lookup, gamma tables
│       ├── config.rs          # TOML configuration and CLI flag parser
│       ├── glidesys.rs        # Glide 3.0 types, enums, bitflags, and vertex layout decoding
│       ├── render.rs          # Frame submission, pipeline selection, and presentation
│       └── utility/           # Vulkan instance/device, bindless texture cache, LFB, Win32, hostmem
├── glide3x/                   # C/stdcall shared library crate (glide3x.dll / libglide3x.so)
│   ├── build.rs               # Generates Win32 .def file for stdcall symbol aliases
│   └── src/
│       ├── lib.rs             # Exported gr* / gu* entrypoints
│       ├── testutil.rs        # Windowed harness, PPM frame dumping, and CLI helpers for demos
│       └── bin/               # Interactive demo, Glide SDK ports (test00..test16), & probes
├── shaders/                   # GLSL 4.50 vertex and fragment shaders (glide.vert, glide.frag)
└── tools/
    ├── enable_wine_dep.py     # Enables NX_COMPAT (DEP) on legacy 32-bit PE executables/DLLs
    └── run_review_checks.py   # Automated ABI, pixel readback, and Vulkan sync validation suite
```

---

## Prerequisites

### Runtime Requirements
- A **Vulkan 1.1+** loader and **Vulkan 1.2+** capable GPU/driver supporting descriptor indexing:
  - `descriptorBindingPartiallyBound`
  - `descriptorBindingSampledImageUpdateAfterBind`
  - `shaderSampledImageArrayNonUniformIndexing`
  - `runtimeDescriptorArray`
- On macOS, **Wine 11+** with **MoltenVK 1.4+** (see [`WINE_MACOS.md`](WINE_MACOS.md)).

### Build Requirements
- **Rust** toolchain (`1.75+`, 2021 edition)
- **`glslc`** shader compiler (from the [Vulkan SDK](https://vulkan.lunarg.com/) or `shaderc`). If `glslc` is not on `PATH`, set `GLSLC=/path/to/glslc`.
- **For 32-bit Windows (`glide3x.dll`) cross-builds**:
  - Rust target `i686-pc-windows-gnu`:
    ```sh
    rustup target add i686-pc-windows-gnu
    ```
  - MinGW-w64 32-bit GCC linker (`i686-w64-mingw32-gcc`) with **DWARF2** exception handling (standard on Linux `mingw-w64` packages; see [`WINE_MACOS.md`](WINE_MACOS.md) for macOS setup).

---

## Building

### Build 32-bit Windows DLL (`glide3x.dll`)

```sh
make build-win
```

Output artifact:
```text
target/i686-pc-windows-gnu/release/glide3x.dll
```

For an unoptimized debug build (configured with compact line-table debug info so Wine can load PE sections cleanly):

```sh
make build-win RELEASE_TYPE=debug
```

### Build Native Host Libraries & Demo Binaries

```sh
make build
```

Output artifacts:
- Linux: `target/release/libglide3x.so` and demo binaries under `target/release/`
- macOS: `target/release/libglide3x.dylib` and demo binaries under `target/release/`
- Windows: `target/release/glide3x.dll`

---

## Installation & Usage

1. Copy `glide3x.dll` into the game's installation directory (next to the game executable).
2. *(Optional)* Copy [`glidevk.toml.example`](glidevk.toml.example) to `glidevk.toml` in the game directory and customize your display or rendering settings.
3. Launch the game with its Glide renderer flag (commonly `-3dfx`):

   **Native Windows:**
   ```powershell
   .\app.exe -3dfx -w
   ```

   **Wine (Linux / macOS):**
   ```sh
   WINEPREFIX="/path/to/wine-prefix" WINEDLLOVERRIDES="glide3x=n,b" \
     wine ./app.exe -3dfx -w
   ```

> **Tip for Wine 11 users:** Wine 11 disables process-wide DEP if *any* loaded 32-bit `.exe` or `.dll` lacks the PE `NX_COMPAT` flag, which can cause multi-second stalls when writing to mapped Vulkan buffers. Run `python3 tools/enable_wine_dep.py "/path/to/copied-game-dir"` against a copy of the game directory to enable `NX_COMPAT` with automatic backups in `before-dep/`. See [`WINE_MACOS.md`](WINE_MACOS.md) for details.

### Runtime Controls & Diagnostics

| Feature | Usage |
|---|---|
| **Pause Rendering (Windows/Wine)** | Press `Ctrl+Shift+F12` while the game window is focused to toggle GPU rendering/presentation on or off (throttles the game loop to 25 Hz while paused). |
| **Disable Cursor Clipping** | Pass `-dxnoclipcursor` (case-insensitive) on the game's command line or set `clip_cursor = false` in `glidevk.toml`. |
| **Logging** | Set `RUST_LOG=info` (or `RUST_LOG=glide3x=debug,glidevk=debug` / `trace` for per-call Glide API tracing). |

---

## Configuration (`glidevk.toml`)

GlideVK reads `glidevk.toml` from the working directory at `grGlideInit` time, falling back to defaults if the file is absent. See [`glidevk.toml.example`](glidevk.toml.example) for a complete template.

### `[window]`

| Key | Type / Options | Default | Description |
|---|---|---|---|
| `windowed` | `bool` | `false` | Force windowed mode instead of fullscreen. |
| `keep_aspect_ratio` | `bool` | `true` | Preserve 4:3 aspect ratio with black pillarbox/letterbox bars when scaled. |
| `static_size` | `"none"`, `"vga"` (`640x480`), `"svga"` (`800x600`), `"xga"` (`1024x768`), `"sxga"` (`1280x960`), `"uxga"` (`1600x1200`), `"qxga"` (`1920x1440`), `"desktop"`, or `{ custom = { width = W, height = H } }` | `"none"` | Override window/output dimensions while keeping the game's internal Glide resolution intact. |
| `resizable` | `bool` | `false` | Allow free window resizing in windowed mode (scales output and remaps mouse input). |
| `centered` | `bool` | `true` | Center the window on the monitor in windowed mode. |
| `clip_cursor` | `bool` | `true` | Confine the OS cursor to the game window's client area while focused in windowed mode (overridden by `-dxnoclipcursor`). |

### `[texture]`

| Key | Type / Options | Default | Description |
|---|---|---|---|
| `memory` | `"mb2"`, `"mb4"`, `"mb8"`, `"mb16"`, `"mb32"`, `"mb64"`, `"mb128"`, `"mb256"`, or `{ custom = N }` | `"mb8"` | Amount of TMU texture address space reported to the application via `grTexMaxAddress` / `grGet`. |

### `[render]`

| Key | Type / Options | Default | Description |
|---|---|---|---|
| `filtering` | `"linear"`, `"bilinear"`, `"cubic"` | `"linear"` | Presentation upscale filter: `"linear"` (sharp pixels with anti-aliased texel edges), `"bilinear"` (hardware linear), or `"cubic"` (9-tap Catmull-Rom bicubic). |
| `fps` | `"off"`, `"top-left"`, `"top-right"`, `"bottom-left"`, `"bottom-right"` | `"off"` | Corner position for the built-in 7-segment FPS counter overlay. |
| `wine_host_memory_workaround` | `bool` | `true` | Import `VirtualAlloc` host memory via `VK_EXT_external_memory_host` on 32-bit Wine to prevent `vkFreeMemory` address-space leaks. |

---

## Testing & Demo Binaries

### Unit Tests

Run the library unit test suite (95 tests covering vertex layouts, ABI flags, texture/palette decoding, configuration parsing, and batching):

```sh
make test
# or:
cargo test --workspace --lib
```

### Interactive & Headless Demos

Run the interactive multi-triangle demo natively:

```sh
make run-test
```

`glide3x` also includes Rust ports of the original 3Dfx Glide SDK tests (`test00` through `test15`), a textured readback test (`test16`), and specialized regression probes:

| Binary | Description |
|---|---|
| `test` | Interactive animated multi-triangle demo |
| `test00` – `test04` | Buffer clear, points, lines, and Gouraud-shaded triangles |
| `test05` – `test06` | Interpenetrating triangles with Z-buffering and W-buffering |
| `test07` – `test10` | Alpha blending, fog tables, chromakeying, and backface culling |
| `test11` – `test12` | Linear framebuffer (`grLfbReadRegion`, `grLfbLock`, write modes) |
| `test13` – `test15` | Iterated alpha, depth functions, and clip rectangles (`grClipWindow`) |
| `test16` | Batched texture upload with framebuffer pixel verification |
| `reopen` | Repeated context open/close and texture churn probe for 32-bit address-space tracking |
| `cursor` | Win32 scaled/letterboxed mouse coordinate mapping and cursor-clipping regression |
| `review_probe` | Multi-mode GPU verification probe (`partial`, `mixed`, `frames`, `state`, `textures`, `capacity`, `updates`, `samplers`, `locks-ui`, `scaled`) |

Most demo binaries accept `-n <frames>` for bounded runs, `-d <path.ppm>` to dump rendered frames, and `--visible` when running under Wine on macOS.

### Full GPU & Synchronization Validation Suite

With a display server (e.g., X11) and `VK_LAYER_KHRONOS_validation` installed, run the end-to-end ABI, guard-page, pixel readback, and Vulkan synchronization-validation checks:

```sh
cargo build --workspace
python3 tools/run_review_checks.py
python3 tools/enable_wine_dep.py --self-test
```

---

## Known Limitations

- **Mipmapping & LOD**: Mipmap chains are sampled with nearest-mip filtering up to the first missing level; trilinear LOD blending, `grTexLodBiasValue` application, and source-only LOD range remapping are not yet implemented.
- **W-Buffer Clears**: Near and far W-buffer clear extremes map accurately; intermediate clear values use a normalized linear approximation rather than 3Dfx's 16-bit floating-point depth encoding.
- **LFB Targets**: `grLfbLock` supports front and back color buffers in standard 16-bit and 32-bit RGB/ARGB formats; auxiliary/depth buffer locks and RLE-compressed LFB writes return `FXFALSE`.

Additional implementation notes and validation details are documented in [`REVIEW_FIXES.md`](REVIEW_FIXES.md).
