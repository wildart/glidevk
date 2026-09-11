# September 2026 review fixes

The numbered findings refer to the September 10–11 whole-project review.
The implementation keeps legacy Glide UI alpha-combine ZERO compatibility behavior.

| Findings | Change | Regression evidence |
|---|---|---|
| 1–2, 37 | Individual exports decode the active layout; ABI flags and extensible values use integer newtypes. Decode Z, Q, both TMUs, and the selected color representation separately. Continuations retain two vertices and winding parity. | Guard-page child process, layout/ABI unit tests, continuation winding test |
| 3, 6–7 | Wait for readers before destruction or descriptor replacement; reclaim before allocation. Split draws at 2,046 active textures, accounting for placeholder and display slots. | `capacity`: 4,200 distinct textures over two frames, including an oversized single-frame working set |
| 4–5 | Render into owned logical color/depth images with explicit dependencies. Presentation samples the front image; readback never accesses a presented swapchain image. | Synchronization validation on all GPU probes and demos |
| 8–11 | Region uploads preserve other pixels; back-buffer writes wait for swap. LFB locks/readback use logical front/back images independently of presentation scaling. | `partial`, `mixed`, `locks-ui`, pillarbox and letterbox probes |
| 12–14, 27–28 | Capture clipping, depth/color masks, comparisons, blend factors, and alpha test per draw. State changes break draw calls within one submission. | `state`, draw-batch unit check, `test15` outside-clip pixel |
| 15–17 | Divide interpolated texture coordinates by interpolated Q; resolve descriptors into upload storage and consume batches. Flush preceding draws before texture/palette mutation. | Source review, `textures`, `updates`, consecutive swaps |
| 18–23 | Preserve mip levels, inclusive partial rows, address zero, and explicit source binding. Capture filter/wrap/mipmap state and both TMU combine operations. | `updates`, `samplers`, `textures`, memory-size checks |
| 24–26 | Apply combine inversion, configured chromakey, and draw-time fog modes/tables. | `state`, `textures`, vendor fog-table equation review |
| 29–30 | Decode accepted packed/NCC formats; distinguish ordinary opaque palettes/intensity from extended palette alpha. | Known one-pixel channel tests and palette mutation probe |
| 31 | Apply gamma to the composed image during presentation, including clears and LFB output. Compensate for automatic sRGB target encoding. | Shader/render-path review; logical LFB reads remain pre-display-gamma |
| 32–36 | Grow vertex storage; honor opening origin, tolerate refresh requests, transform clip coordinates, and query the active logical viewport. | 66,000-vertex batch, clip-coordinate checks, non-60-Hz probe opening, exported query tests |
| 38–40 | Cargo compiles shaders into `OUT_DIR`; either compiler failure fails the build. Make targets use actual Cargo inputs/output and host shader compilation. | Clean source build without SPIR-V; failing compiler mock; native/Windows builds |
| 41–43 | Pass native HWND in Windows demos. Pixel/dump/validation failures fail the process. Test the actual exported query implementation, including short buffers. | Wine `test16`, GPU runner, exported query tests |
| 44–46 | Stop advertising inactive settings, share the checked-in configuration template, correct lowercase custom examples, and obtain Windows desktop dimensions from the native monitor. | Configuration literal tests, Windows build and source review |

Follow-up review also fixed noncanonical `FxBool`, LFB NOIDLE/RLE input handling,
reserved blend values, the FBI constant, raw `TexInfo` fields, and the
`grTexDownloadMipMapLevelPartial` return type. Unsupported compressed/depth LFB
formats and non-front/back lock targets return failure. Packed color locks preload existing pixels.

Cleanup removes unused dependency entries, duplicate binary declarations, the
private eager upload path, stale tutorial shader artifacts, and the unused
utility Makefile. Public setup/configuration/query helpers remain; public texture
upload helpers delegate to the shared uploader. Rust callers of LFB readback now
need mutable access so pending draws can be submitted in order.

## Run the checks

Install Rust, a Vulkan loader/driver, and `glslc` from shaderc or the Vulkan SDK.
Set `GLSLC` if the compiler is not on `PATH`. Shader compilation runs on the
build host, including Windows cross builds; it does not rely on a `cross` image.
GNU Windows builds also need the selected Rust target and matching MinGW linker.

```sh
cargo build --locked --workspace
cargo test --locked --workspace --lib
python3 tools/run_review_checks.py
make build-win RELEASE_TYPE=debug
wine target/i686-pc-windows-gnu/debug/test16.exe -n 3
```

The GPU runner needs a display (defaults to X11 `:1` on Linux) and enables
synchronization validation. Install Vulkan validation layers to check hazards.
Use `--target-dir` when Cargo artifacts live outside `target/`.

Validation passed 91 unit tests on Linux and native macOS/MoltenVK, all 16 runner
checks on Linux with synchronization validation, a complete i686 Windows build,
and the Windows texture demo under Wine. Release-mode native MoltenVK also passed
sampler and ten-frame presentation probes. A source-only copy built offline
without generated shader files.

## Remaining limits

Submissions are synchronous at resource mutations, clears, LFB operations, and
presentation. Pipeline/scissor changes within ordinary drawing stay in a batch.
LFB staging allocations are per operation. Profile before adding fence-owned
resource generations or a staging pool.

Mip chains stop at the first missing level until that level is downloaded.
The uploaded chain determines the effective LOD range; source-only LOD range
changes, trilinear blending, and LOD bias remain unsupported.

W-buffer near/far clears are supported; intermediate clear values still use the
existing normalized approximation rather than 3Dfx's 16-bit floating encoding.
The UI regression checks the preserved alpha-combine ZERO behavior synthetically;
this pass does not certify a complete game session, native Windows hardware,
or resize behavior.

The native macOS debug window probe stops inside the existing winit/icrate
Objective-C signature verifier before renderer initialization. Release-mode
window probes and the native library unit suite pass.
