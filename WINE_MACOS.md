# 32-bit Glide games on Apple silicon with Wine

Keep the Windows build as `i686-pc-windows-gnu`: legacy 32-bit Windows games
load a 32-bit `glide3x.dll`. Tested with Wine 11.0 and MoltenVK 1.4.1 on Apple
silicon.

## Build locally

Run from this repository's root:

```sh
make build-win
```

The release DLL is `target/i686-pc-windows-gnu/release/glide3x.dll`.
Cross-compiling requires Rust's `i686-pc-windows-gnu` target, `glslc`, and a
MinGW toolchain. Homebrew's default i686 MinGW uses SJLJ exceptions, while Rust
requires a GNU DWARF2 exception runtime; configure Cargo's
`[target.i686-pc-windows-gnu]` linker in `.cargo/config.toml` accordingly when
building on macOS.

## Remove animation stalls

Close the game and run this against a **copy** of its installation:

```sh
python3 tools/enable_wine_dep.py "/path/to/game"
```

The script enables `NX_COMPAT` on 32-bit `.exe` and `.dll` files in the game
directory, preserving each modified original in `before-dep/`. Already-enabled
files are unchanged; existing backups are never overwritten. To undo the change,
close the game and copy the files in `before-dep/` back into the game directory.

Wine 11 disables DEP when it loads **any** module without `NX_COMPAT`, so
changing only the main executable is insufficient. Without DEP, Wine forces
execute permission on readable memory. On Apple silicon under Wine, this
produced approximately 2.25-second stalls when first copying data into mapped
Vulkan buffers. Clearing `NX_COMPAT` on the otherwise-fast `test16.exe`
reproduced the stalls. Enabling it on the executable and all legacy DLLs removed
the animation stalls, with sound enabled. No texture-cache change was needed.

## Launch

Put the Windows `glide3x.dll` beside the game executable, then run from that
directory:

```sh
WINEPREFIX="/path/to/wine-prefix" WINEDLLOVERRIDES="glide3x=n,b" \
  wine ./app.exe -3dfx -w
```

## Checks

```sh
python3 tools/enable_wine_dep.py --self-test
make build-win
wine target/i686-pc-windows-gnu/release/test16.exe -n 3
wine target/i686-pc-windows-gnu/release/test04.exe -d triangle.ppm --visible
```

`test16` checks texture colors through framebuffer readback after a
750-vertex upload. Use `--visible` when dumping frames through Wine on macOS:
a hidden window may never receive redraw events.
