//! Standalone Glide API functions that don't require a Gr instance
//!
//! These functions are implemented here and re-exported by glide3x with
//! the appropriate calling convention for DLL exports.

use crate::glidesys::*;

// Static strings for grGetString
static VERSION: &[u8] = b"3.0\0";
static VENDOR: &[u8] = b"3Dfx Interactive\0";
static RENDERER: &[u8] = b"Glide\0";
static HARDWARE: &[u8] = b"Voodoo2\0";
static EXTENSION: &[u8] = b" \0";

/// GrProc type - function pointer for extension functions
pub type GrProc = Option<unsafe extern "C" fn()>;

/// Color combine function (stub - always uses constant color in current implementation)
pub fn color_combine(
    _function: CombineFunction,
    _factor: CombineFactor,
    _local: CombineLocal,
    _other: CombineOther,
    _invert: FxBool,
) {
    // Current implementation always uses constant color for drawing
    // Full color combine support would require shader modifications
}

/// Alpha combine function (stub - always uses constant alpha in current implementation)
pub fn alpha_combine(
    _function: CombineFunction,
    _factor: CombineFactor,
    _local: CombineLocal,
    _other: CombineOther,
    _invert: FxBool,
) {
    // Current implementation always uses constant alpha for drawing
    // Full alpha combine support would require shader modifications
}

/// Coordinate space function (stub - current implementation uses window coordinates)
pub fn coordinate_space(_mode: CoordinateSpaceMode) {
    // Current implementation always uses window coordinates
    // Clip coordinates would require transformation matrix support
}


/// Finish function - waits for all rendering to complete
pub fn finish() {
    // In Vulkan, this would wait for the GPU to finish all pending operations
    // Current implementation handles synchronization internally during buffer swap
}

/// GetString function - retrieves Glide string information
pub fn get_string(pname: EnvironmentInfo) -> *const std::ffi::c_char {
    match pname {
        EnvironmentInfo::Version => VERSION.as_ptr() as *const std::ffi::c_char,
        EnvironmentInfo::Vendor => VENDOR.as_ptr() as *const std::ffi::c_char,
        EnvironmentInfo::Renderer => RENDERER.as_ptr() as *const std::ffi::c_char,
        EnvironmentInfo::Hardware => HARDWARE.as_ptr() as *const std::ffi::c_char,
        EnvironmentInfo::Extension => EXTENSION.as_ptr() as *const std::ffi::c_char,
    }
}

/// Get function - retrieves various Glide parameters
/// Reference: docs/glide3x/cvg/glide3/src/diget.c
pub fn get(pname: GrGetParam, plength: FxU32, params: *mut FxI32) -> FxU32 {
    if params.is_null() {
        return 0;
    }

    unsafe {
        match pname {
            // Depth buffer bits (16-bit)
            GrGetParam::BitsDepth => {
                if plength >= 4 {
                    *params = 16; // SST1_BITS_DEPTH
                    4
                } else {
                    0
                }
            }
            // RGBA bits (5, 6, 5, 0 for RGB565)
            GrGetParam::BitsRgba => {
                if plength >= 16 {
                    *params = 5; // R
                    *params.add(1) = 6; // G
                    *params.add(2) = 5; // B
                    *params.add(3) = 0; // A (no alpha in 565)
                    16
                } else {
                    0
                }
            }
            // FIFO fullness (not applicable in Vulkan)
            GrGetParam::FifoFullness => {
                if plength >= 8 {
                    *params = 0; // Empty FIFO
                    *params.add(1) = 0;
                    8
                } else {
                    0
                }
            }
            // Fog table entries
            GrGetParam::FogTableEntries => {
                if plength >= 4 {
                    *params = 64; // kInternalFogTableEntryCount
                    4
                } else {
                    0
                }
            }
            // Gamma table entries
            GrGetParam::GammaTableEntries => {
                if plength >= 4 {
                    *params = 256; // VOODOO_GAMMA_TABLE_SIZE
                    4
                } else {
                    0
                }
            }
            // Glide state structure size
            GrGetParam::GlideStateSize => {
                if plength >= 4 {
                    *params = 1024; // Approximate size
                    4
                } else {
                    0
                }
            }
            // Vertex layout structure size
            GrGetParam::GlideVertexLayoutSize => {
                if plength >= 4 {
                    *params = 256; // Approximate size
                    4
                } else {
                    0
                }
            }
            // Hardware busy flag (always ready in Vulkan)
            GrGetParam::IsBusy => {
                if plength >= 4 {
                    *params = 0; // FXFALSE - not busy
                    4
                } else {
                    0
                }
            }
            // LFB pixel pipe availability
            GrGetParam::LfbPixelPipe => {
                if plength >= 4 {
                    *params = 1; // FXTRUE - available (Voodoo2 compatible)
                    4
                } else {
                    0
                }
            }
            // Maximum texture size
            GrGetParam::MaxTextureSize => {
                if plength >= 4 {
                    *params = 256; // 256x256 max texture size
                    4
                } else {
                    0
                }
            }
            // Maximum texture aspect ratio (3 = 8:1)
            GrGetParam::MaxTextureAspectRatio => {
                if plength >= 4 {
                    *params = 3; // 8:1 aspect ratio (GR_ASPECT_LOG2_8x1)
                    4
                } else {
                    0
                }
            }
            // Frame buffer memory (4MB)
            GrGetParam::MemoryFb => {
                if plength >= 4 {
                    *params = 4 * 1024 * 1024; // 4MB frame buffer
                    4
                } else {
                    0
                }
            }
            // Texture memory per TMU (4MB)
            GrGetParam::MemoryTmu => {
                if plength >= 4 {
                    *params = 4 * 1024 * 1024; // 4MB texture memory
                    4
                } else {
                    0
                }
            }
            // UMA memory (not applicable - non-UMA architecture)
            GrGetParam::MemoryUma => {
                if plength >= 4 {
                    *params = 0; // Non-UMA architecture
                    4
                } else {
                    0
                }
            }
            // Number of boards
            GrGetParam::NumBoards => {
                if plength >= 4 {
                    *params = 1; // Always report 1 board
                    4
                } else {
                    0
                }
            }
            // Non-power-of-two textures support
            GrGetParam::NonPowerOfTwoTextures => {
                if plength >= 4 {
                    *params = 0; // FXFALSE - not supported
                    4
                } else {
                    0
                }
            }
            // Number of frame buffers (1 for non-SLI)
            GrGetParam::NumFb => {
                if plength >= 4 {
                    *params = 1; // No SLI
                    4
                } else {
                    0
                }
            }
            // Number of swap history buffer entries
            GrGetParam::NumSwapHistoryBuffer => {
                if plength >= 4 {
                    *params = 8; // Voodoo2/Banshee swap history size
                    4
                } else {
                    0
                }
            }
            // Number of TMUs
            GrGetParam::NumTmu => {
                if plength >= 4 {
                    *params = 1; // Report 1 TMU
                    4
                } else {
                    0
                }
            }
            // Pending buffer swaps
            GrGetParam::PendingBufferswaps => {
                if plength >= 4 {
                    *params = 0; // No pending swaps
                    4
                } else {
                    0
                }
            }
            // Frame buffer revision
            GrGetParam::RevisionFb => {
                if plength >= 4 {
                    *params = 2; // Voodoo2 revision
                    4
                } else {
                    0
                }
            }
            // TMU revision
            GrGetParam::RevisionTmu => {
                if plength >= 4 {
                    *params = 1; // TMU revision
                    4
                } else {
                    0
                }
            }
            // Statistics - lines drawn
            GrGetParam::StatsLines => {
                if plength >= 4 {
                    *params = 0; // Stats not tracked
                    4
                } else {
                    0
                }
            }
            // Statistics - pixels failed alpha test
            GrGetParam::StatsPixelsAfuncFail => {
                if plength >= 4 {
                    *params = 0;
                    4
                } else {
                    0
                }
            }
            // Statistics - pixels failed chroma test
            GrGetParam::StatsPixelsChromaFail => {
                if plength >= 4 {
                    *params = 0;
                    4
                } else {
                    0
                }
            }
            // Statistics - pixels failed depth test
            GrGetParam::StatsPixelsDepthfuncFail => {
                if plength >= 4 {
                    *params = 0;
                    4
                } else {
                    0
                }
            }
            // Statistics - pixels in
            GrGetParam::StatsPixelsIn => {
                if plength >= 4 {
                    *params = 0;
                    4
                } else {
                    0
                }
            }
            // Statistics - pixels out
            GrGetParam::StatsPixelsOut => {
                if plength >= 4 {
                    *params = 0;
                    4
                } else {
                    0
                }
            }
            // Statistics - pixels (for reset)
            GrGetParam::StatsPixels => {
                if plength >= 4 {
                    *params = 0;
                    4
                } else {
                    0
                }
            }
            // Statistics - points drawn
            GrGetParam::StatsPoints => {
                if plength >= 4 {
                    *params = 0;
                    4
                } else {
                    0
                }
            }
            // Statistics - triangles in
            GrGetParam::StatsTrianglesIn => {
                if plength >= 4 {
                    *params = 0;
                    4
                } else {
                    0
                }
            }
            // Statistics - triangles out
            GrGetParam::StatsTrianglesOut => {
                if plength >= 4 {
                    *params = 0;
                    4
                } else {
                    0
                }
            }
            // Statistics - triangles (for reset)
            GrGetParam::StatsTriangles => {
                if plength >= 4 {
                    *params = 0;
                    4
                } else {
                    0
                }
            }
            // Swap history (8 entries)
            GrGetParam::SwapHistory => {
                if plength >= 32 {
                    for i in 0..8 {
                        *params.add(i) = 0;
                    }
                    32
                } else {
                    0
                }
            }
            // Passthru support
            GrGetParam::SupportsPassthru => {
                if plength >= 4 {
                    *params = 1; // FXTRUE
                    4
                } else {
                    0
                }
            }
            // Texture alignment
            GrGetParam::TextureAlign => {
                if plength >= 4 {
                    *params = 8; // SST_TEXTURE_ALIGN
                    4
                } else {
                    0
                }
            }
            // Video position (H/V retrace)
            GrGetParam::VideoPosition => {
                if plength >= 8 {
                    *params = 0; // V position
                    *params.add(1) = 0; // H position
                    8
                } else {
                    0
                }
            }
            // Viewport (x, y, width, height)
            GrGetParam::Viewport => {
                if plength >= 16 {
                    *params = 0; // x
                    *params.add(1) = 0; // y
                    *params.add(2) = 640; // width (default)
                    *params.add(3) = 480; // height (default)
                    16
                } else {
                    0
                }
            }
            // W depth min/max
            GrGetParam::WdepthMinMax => {
                if plength >= 8 {
                    // W depth: nearest = 0x0000, farthest = 0xFFFF
                    *params = 0x0000; // nearest (SST1_WDEPTHVALUE_NEAREST)
                    *params.add(1) = 0xFFFF_u32 as i32; // farthest (SST1_WDEPTHVALUE_FARTHEST)
                    8
                } else {
                    0
                }
            }
            // Z depth min/max
            GrGetParam::ZdepthMinMax => {
                if plength >= 8 {
                    // Z depth: nearest = 0xFFFF, farthest = 0x0000
                    *params = 0xFFFF_u32 as i32; // nearest (SST1_ZDEPTHVALUE_NEAREST)
                    *params.add(1) = 0x0000; // farthest (SST1_ZDEPTHVALUE_FARTHEST)
                    8
                } else {
                    0
                }
            }
            // Vertex parameter (for reset)
            GrGetParam::VertexParameter => {
                if plength >= 4 {
                    *params = 0;
                    4
                } else {
                    0
                }
            }
            // Gamma bits
            GrGetParam::BitsGamma => {
                if plength >= 4 {
                    *params = 8; // 8-bit gamma
                    4
                } else {
                    0
                }
            }
        }
    }
}

/// GetProcAddress function - retrieves extension function pointers
pub fn get_proc_address(proc_name: *const std::ffi::c_char) -> GrProc {
    // Log what function is being requested
    if !proc_name.is_null() {
        let name = unsafe { std::ffi::CStr::from_ptr(proc_name) };
        log::debug!("grGetProcAddress({:?}) -> NULL", name);
    }
    // No extensions supported - always return NULL
    None
}

/// Load gamma correction table
/// The table has 256 entries, each a 32-bit value with R, G, B components
pub fn load_gamma_table(
    _num_entries: FxU32,
    _red: *const FxU32,
    _green: *const FxU32,
    _blue: *const FxU32,
) {
    // Stub - gamma correction not yet implemented in Vulkan backend
    // In a full implementation, this would update the swapchain gamma ramp
}

/// Glide utility function: compute and load gamma correction tables
/// Formula: c_gamma = 255 * (c/255)^(1/gamma)
pub fn gamma_correction_rgb(gamma_red: f32, gamma_green: f32, gamma_blue: f32) {
    // Build gamma correction tables (256 entries each)
    let mut red_table: [FxU32; 256] = [0; 256];
    let mut green_table: [FxU32; 256] = [0; 256];
    let mut blue_table: [FxU32; 256] = [0; 256];

    for x in 0..256 {
        // Original formula from gamma.c:
        // gammaTable[x] = FTOL(POW(x/255.0F, 1.0F/gamma) * 255.0F + 0.5F)
        // The + 0.5 before truncation is equivalent to rounding
        let c = x as f32 / 255.0;

        let r = (c.powf(1.0 / gamma_red) * 255.0 + 0.5) as u32;
        let g = (c.powf(1.0 / gamma_green) * 255.0 + 0.5) as u32;
        let b = (c.powf(1.0 / gamma_blue) * 255.0 + 0.5) as u32;

        red_table[x] = r.min(255);
        green_table[x] = g.min(255);
        blue_table[x] = b.min(255);
    }

    // Download the gamma table to hardware
    load_gamma_table(
        256,
        red_table.as_ptr(),
        green_table.as_ptr(),
        blue_table.as_ptr(),
    );
}
