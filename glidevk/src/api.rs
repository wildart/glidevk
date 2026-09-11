//! Standalone Glide API functions that don't require a Gr instance
//!
//! These functions are implemented here and re-exported by glide3x with
//! the appropriate calling convention for DLL exports.

use crate::glidesys::*;
use std::sync::atomic::{AtomicU32, Ordering};

/// Global per-channel gamma values. The standalone Glide gamma functions cannot
/// access a `Gr` instance, so rendering reads these atomics when building push
/// constants.
pub static GAMMA_RGB: [AtomicU32; 3] = [
    AtomicU32::new(1.0f32.to_bits()),
    AtomicU32::new(1.0f32.to_bits()),
    AtomicU32::new(1.0f32.to_bits()),
];

// Static strings for grGetString
static VERSION: &[u8] = b"3.0\0";
static VENDOR: &[u8] = b"3Dfx Interactive\0";
static RENDERER: &[u8] = b"Glide\0";
static HARDWARE: &[u8] = b"Voodoo2\0";
static EXTENSION: &[u8] = b" \0";

/// GrProc type - function pointer for extension functions
pub type GrProc = Option<unsafe extern "C" fn()>;

static COORDINATE_SPACE: AtomicU32 = AtomicU32::new(0);
pub fn coordinate_space(mode: CoordinateSpaceMode) {
    COORDINATE_SPACE.store(mode as u32, Ordering::Relaxed);
}
pub fn current_coordinate_space() -> u32 {
    COORDINATE_SPACE.load(Ordering::Relaxed)
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
/// # Safety
/// `params` must point to a valid buffer of at least `plength` bytes, or be null.
/// Reference: docs/glide3x/cvg/glide3/src/diget.c
pub unsafe fn get(pname: GrGetParam, plength: FxU32, params: *mut FxI32) -> FxU32 {
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
/// # Safety
/// `proc_name` must be a valid null-terminated C string or null.
pub unsafe fn get_proc_address(proc_name: *const std::ffi::c_char) -> GrProc {
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
    num_entries: FxU32,
    red: *const FxU32,
    green: *const FxU32,
    blue: *const FxU32,
) {
    if num_entries < 2 || (red.is_null() && green.is_null() && blue.is_null()) {
        return;
    }

    // The Vulkan backend currently represents a gamma table with a per-channel
    // exponent. Estimate that exponent from each supplied LUT independently.
    let mut gamma = current_gamma_rgb();
    for (channel, table_ptr) in [red, green, blue].into_iter().enumerate() {
        if !table_ptr.is_null() {
            let table = unsafe { std::slice::from_raw_parts(table_ptr, num_entries as usize) };
            if let Some(estimated) = estimate_gamma_from_table(table) {
                gamma[channel] = estimated;
            }
        }
    }

    set_gamma_rgb(gamma);
    log::debug!(
        "Gamma table loaded: estimated R={:.3} G={:.3} B={:.3}",
        gamma[0],
        gamma[1],
        gamma[2]
    );
}

/// Glide utility function: compute and load gamma correction tables
/// Formula: c_gamma = 255 * (c/255)^(1/gamma)
pub fn gamma_correction_rgb(gamma_red: f32, gamma_green: f32, gamma_blue: f32) {
    let gamma = [gamma_red, gamma_green, gamma_blue].map(sanitize_gamma);
    set_gamma_rgb(gamma);
    log::debug!(
        "Gamma set: R={:.3} G={:.3} B={:.3}",
        gamma[0],
        gamma[1],
        gamma[2]
    );
}

pub fn current_gamma_rgb() -> [f32; 3] {
    GAMMA_RGB
        .each_ref()
        .map(|value| f32::from_bits(value.load(Ordering::Relaxed)))
}

fn set_gamma_rgb(gamma: [f32; 3]) {
    for (target, value) in GAMMA_RGB.iter().zip(gamma.map(sanitize_gamma)) {
        target.store(value.to_bits(), Ordering::Relaxed);
    }
}

fn sanitize_gamma(gamma: f32) -> f32 {
    if gamma.is_finite() && gamma > 0.0 {
        gamma.clamp(0.1, 10.0)
    } else {
        1.0
    }
}

fn estimate_gamma_from_table(table: &[FxU32]) -> Option<f32> {
    if table.len() < 2 {
        return None;
    }

    let max_index = table.len() - 1;
    let sample_index = max_index / 2;
    let input = sample_index as f64 / max_index as f64;
    let output = table[sample_index].min(255) as f64 / 255.0;

    if input <= 0.0 || input >= 1.0 || output <= 0.0 || output >= 1.0 {
        return None;
    }

    Some(sanitize_gamma((input.ln() / output.ln()) as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_gamma_from_generated_table() {
        let expected = 1.8f32;
        let table: Vec<FxU32> = (0..256)
            .map(|value| {
                let normalized = value as f32 / 255.0;
                (normalized.powf(1.0 / expected) * 255.0 + 0.5) as FxU32
            })
            .collect();

        let estimated = estimate_gamma_from_table(&table).unwrap();
        assert!((estimated - expected).abs() < 0.03);
    }

    #[test]
    fn rejects_degenerate_gamma_tables() {
        assert_eq!(estimate_gamma_from_table(&[]), None);
        assert_eq!(estimate_gamma_from_table(&[0, 255]), None);
        assert_eq!(estimate_gamma_from_table(&[255; 256]), None);
    }
}
