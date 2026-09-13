#![allow(non_snake_case, dead_code)]

pub mod api;
pub mod config;
pub mod glidesys;
mod render;
pub mod utility;

use ash::vk;
use std::ffi::CStr;
use std::os::raw::c_char;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use glidesys::*;
use utility::vulkan::{VulkanApp, VulkanInstance};
#[cfg(target_os = "linux")]
use utility::window::WindowInteraction;

use crate::utility::vulkan;

const GLIDE_NUM_TMU: usize = 2;
const EXTENSION: &[u8] = b" \0";
const HARDWARE: &[u8] = b"Banshee\0";
const RENDERER: &[u8] = b"Glide\0";
const VENDOR: &[u8] = b"3Dfx Interactive\0";
const VERSION: &[u8] = b"3.0\0";

// --- FPS overlay helpers ---

fn fps_digits(mut fps: u32) -> Vec<u8> {
    if fps == 0 {
        return vec![0];
    }
    let mut digits = Vec::new();
    while fps > 0 {
        digits.push((fps % 10) as u8);
        fps /= 10;
    }
    digits.reverse();
    digits
}

fn push_quad(
    vertices: &mut Vec<utility::vertex::GpuVertex>,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: [f32; 4],
) {
    // CF_LOCAL(1) for both color and alpha, all other fields 0
    let fps_flags: u32 = 1 | (1 << 5);
    let v = |x: f32, y: f32| utility::vertex::GpuVertex {
        position: [x, y],
        color,
        tex_coord: [0.0, 0.0],
        oow: 1.0,
        constant_color: color,
        combine_flags: fps_flags,
        texture_index: 0,
        ..Default::default()
    };
    vertices.push(v(x1, y1));
    vertices.push(v(x2, y1));
    vertices.push(v(x1, y2));
    vertices.push(v(x2, y1));
    vertices.push(v(x2, y2));
    vertices.push(v(x1, y2));
}

/// Draw a digit (0-9) as a 7-segment display using colored quads
fn draw_7seg_digit(
    vertices: &mut Vec<utility::vertex::GpuVertex>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    digit: u8,
    color: [f32; 4],
) {
    // Segments: top, top-left, top-right, middle, bottom-left, bottom-right, bottom
    //  _        segment 0
    // |_|       segments 1,3,2
    // |_|       segments 4,6,5
    let t = 1.5; // segment thickness
    let mid = h / 2.0;

    // Segment bitmask per digit: bits = 0:top 1:top-left 2:top-right 3:mid 4:bot-left 5:bot-right 6:bot
    let segments: [u8; 10] = [
        0b1110111, // 0
        0b0000110, // 1
        0b1011101, // 2
        0b1001111, // 3
        0b0101110, // 4
        0b1101011, // 5
        0b1111011, // 6
        0b0000111, // 7
        0b1111111, // 8
        0b1101111, // 9
    ];
    let s = segments[digit as usize % 10];

    // Top horizontal
    if s & (1 << 0) != 0 {
        push_quad(vertices, x, y, x + w, y + t, color);
    }
    // Top-left vertical
    if s & (1 << 1) != 0 {
        push_quad(vertices, x, y, x + t, y + mid, color);
    }
    // Top-right vertical
    if s & (1 << 2) != 0 {
        push_quad(vertices, x + w - t, y, x + w, y + mid, color);
    }
    // Middle horizontal
    if s & (1 << 3) != 0 {
        push_quad(
            vertices,
            x,
            y + mid - t / 2.0,
            x + w,
            y + mid + t / 2.0,
            color,
        );
    }
    // Bottom-left vertical
    if s & (1 << 4) != 0 {
        push_quad(vertices, x, y + mid, x + t, y + h, color);
    }
    // Bottom-right vertical
    if s & (1 << 5) != 0 {
        push_quad(vertices, x + w - t, y + mid, x + w, y + h, color);
    }
    // Bottom horizontal
    if s & (1 << 6) != 0 {
        push_quad(vertices, x, y + h - t, x + w, y + h, color);
    }
}

//  error management
extern "C" fn error_callback(string: *const c_char, fatal: FxBool) {
    let qualifier = if fatal.is_true() {
        "Fatal"
    } else {
        "Recoverable"
    };
    // Safety: we are always the producer of this string, so it’s always UTF-8 encoded.
    let string = unsafe { CStr::from_ptr(string) }.to_str().unwrap();
    eprintln!("{qualifier} error: {string}");
}

#[derive(Debug, Clone)]
pub struct State {
    // Buffer configuration
    num_buffers: i32,
    num_aux_buffers: i32,
    color_format: ColorFormat,

    // Clip window
    clipwindowf_xmin: f32,
    clipwindowf_ymin: f32,
    clipwindowf_xmax: f32,
    clipwindowf_ymax: f32,
    /// Game's requested resolution (what the game thinks is the screen size)
    screen_width: u32,
    screen_height: u32,
    /// Actual window/surface size (may differ from screen_width/height due to static_size config)
    window_width: u32,
    window_height: u32,
    /// Viewport offset and size for letterboxing (when keep_aspect_ratio is enabled)
    viewport_x: u32,
    viewport_y: u32,
    viewport_width: u32,
    viewport_height: u32,

    // Depth buffer state
    depth_buffer_mode: DepthBufferMode,
    depth_buffer_function: CmpFnc,
    depth_mask: bool,

    // Color mask state
    color_mask_rgb: bool,
    color_mask_alpha: bool,

    // Alpha blend state
    alpha_blend_rgb_src: AlphaBlendFnc,
    alpha_blend_rgb_dst: AlphaBlendFnc,
    alpha_blend_alpha_src: AlphaBlendFnc,
    alpha_blend_alpha_dst: AlphaBlendFnc,

    // Culling state
    cull_mode: CullMode,

    // Dither state
    dither_mode: DitherMode,

    // Render buffer
    render_buffer: Buffer,

    // Y origin (upper-left or lower-left)
    origin: OriginLocation,

    // Constant color
    constant_color: Color,

    // Color combine state
    color_combine_function: CombineFunction,
    color_combine_factor: CombineFactor,
    color_combine_local: CombineLocal,
    color_combine_other: CombineOther,
    color_combine_invert: bool,

    // Alpha combine state
    alpha_combine_function: CombineFunction,
    alpha_combine_factor: CombineFactor,
    alpha_combine_local: CombineLocal,
    alpha_combine_other: CombineOther,
    alpha_combine_invert: bool,

    // Fog state
    fog_mode: FogMode,
    fog_color: Color,
    fog_table: [u8; 64],

    // Chromakey state
    chromakey_mode: ChromakeyMode,
    chromakey_value: Color,

    // Alpha test state
    alpha_test_function: CmpFnc,
    alpha_test_reference: Alpha,

    // Enable/disable state
    aa_ordered: bool,
    allow_mipmap_dither: bool,
    passthru: bool,
    shameless_plug: bool,
    video_smoothing: bool,

    // Texture state per TMU
    tex_source: [FxU32; GLIDE_NUM_TMU],
    tex_filter_mode_min: [TextureFilterMode; GLIDE_NUM_TMU],
    tex_filter_mode_mag: [TextureFilterMode; GLIDE_NUM_TMU],
    tex_clamp_mode_s: [TextureClampMode; GLIDE_NUM_TMU],
    tex_clamp_mode_t: [TextureClampMode; GLIDE_NUM_TMU],
    tex_mipmap_mode: [MipMapMode; GLIDE_NUM_TMU],
    tex_lod_bias: [f32; GLIDE_NUM_TMU],
    tex_combine_function: [CombineFunction; GLIDE_NUM_TMU],
    tex_combine_factor: [CombineFactor; GLIDE_NUM_TMU],
    tex_combine_alpha_function: [CombineFunction; GLIDE_NUM_TMU],
    tex_combine_alpha_factor: [CombineFactor; GLIDE_NUM_TMU],
    tex_combine_rgb_invert: [bool; GLIDE_NUM_TMU],
    tex_combine_alpha_invert: [bool; GLIDE_NUM_TMU],

    // Texture table state
    tex_table_type: TextureTableType,
    palette_generation: u64,

    // Vertex layout configuration
    vertex_layout: VertexLayout,
}

impl Default for State {
    fn default() -> Self {
        State {
            num_buffers: 0,
            num_aux_buffers: 0,
            color_format: ColorFormat::default(),
            clipwindowf_xmin: 0.0,
            clipwindowf_ymin: 0.0,
            clipwindowf_xmax: 0.0,
            clipwindowf_ymax: 0.0,
            screen_width: 0,
            screen_height: 0,
            window_width: 0,
            window_height: 0,
            viewport_x: 0,
            viewport_y: 0,
            viewport_width: 0,
            viewport_height: 0,
            // Default depth state: disabled, less-or-equal compare, writes enabled
            depth_buffer_mode: DepthBufferMode::Disable,
            depth_buffer_function: CmpFnc::LEqual,
            depth_mask: true,
            // Default color mask: all enabled
            color_mask_rgb: true,
            color_mask_alpha: true,
            // Default alpha blend: one, zero (no blending)
            alpha_blend_rgb_src: AlphaBlendFnc::One,
            alpha_blend_rgb_dst: AlphaBlendFnc::Zero,
            alpha_blend_alpha_src: AlphaBlendFnc::One,
            alpha_blend_alpha_dst: AlphaBlendFnc::Zero,
            // Default culling: disabled
            cull_mode: CullMode::Disable,
            // Default dithering: 4x4
            dither_mode: DitherMode::Dither4x4,
            // Default render buffer: back buffer
            render_buffer: Buffer::Back,
            // Default origin: upper-left
            origin: OriginLocation::UpperLeft,
            // Default constant color: opaque white
            constant_color: Color(0xFFFFFFFF),
            // Default color combine: ScaleOther (texture * iterated)
            color_combine_function: CombineFunction::ScaleOther,
            color_combine_factor: CombineFactor::Local,
            color_combine_local: CombineLocal::Iterated,
            color_combine_other: CombineOther::Texture,
            color_combine_invert: false,
            // Default alpha combine: ScaleOther
            alpha_combine_function: CombineFunction::ScaleOther,
            alpha_combine_factor: CombineFactor::Local,
            alpha_combine_local: CombineLocal::Iterated,
            alpha_combine_other: CombineOther::Iterated,
            alpha_combine_invert: false,
            // Default fog: disabled
            fog_mode: FogMode::Disable,
            fog_color: Color(0x00000000),
            fog_table: [0u8; 64],
            // Default chromakey: disabled
            chromakey_mode: ChromakeyMode::Disable,
            chromakey_value: Color(0x00000000),
            // Default alpha test: always pass
            alpha_test_function: CmpFnc::Always,
            alpha_test_reference: Alpha(0),
            // Default enable/disable state: all disabled
            aa_ordered: false,
            allow_mipmap_dither: false,
            passthru: false,
            shameless_plug: false,
            video_smoothing: false,
            // Default texture state
            tex_source: [u32::MAX; GLIDE_NUM_TMU],
            tex_filter_mode_min: [TextureFilterMode::PointSampled; GLIDE_NUM_TMU],
            tex_filter_mode_mag: [TextureFilterMode::PointSampled; GLIDE_NUM_TMU],
            tex_clamp_mode_s: [TextureClampMode::Wrap; GLIDE_NUM_TMU],
            tex_clamp_mode_t: [TextureClampMode::Wrap; GLIDE_NUM_TMU],
            tex_mipmap_mode: [MipMapMode::Disable; GLIDE_NUM_TMU],
            tex_lod_bias: [0.0; GLIDE_NUM_TMU],
            tex_combine_function: [CombineFunction::Local, CombineFunction::Zero],
            tex_combine_factor: [CombineFactor::Zero; GLIDE_NUM_TMU],
            tex_combine_alpha_function: [CombineFunction::Local, CombineFunction::Zero],
            tex_combine_alpha_factor: [CombineFactor::Zero; GLIDE_NUM_TMU],
            tex_combine_rgb_invert: [false; GLIDE_NUM_TMU],
            tex_combine_alpha_invert: [false; GLIDE_NUM_TMU],
            // Default texture table type
            tex_table_type: TextureTableType::Palette,
            palette_generation: 0,
            // Default vertex layout: all disabled
            vertex_layout: VertexLayout::default(),
        }
    }
}
/// State for LFB (Linear Frame Buffer) lock
struct LfbLockState {
    /// Whether a buffer is currently locked
    is_locked: bool,
    /// The lock type (read/write)
    lock_type: glidesys::LfbLock,
    /// The locked buffer
    buffer: glidesys::Buffer,
    /// The write mode (pixel format)
    write_mode: glidesys::LfbWriteMode,
    /// The origin location (upper-left or lower-left)
    origin: glidesys::OriginLocation,
    /// The allocated buffer data
    data: Vec<u8>,
    /// Width of the locked buffer
    width: u32,
    /// Height of the locked buffer
    height: u32,
    /// Stride in bytes
    stride: u32,
}

impl Default for LfbLockState {
    fn default() -> Self {
        LfbLockState {
            is_locked: false,
            lock_type: glidesys::LfbLock::ReadOnly,
            buffer: glidesys::Buffer::Back,
            write_mode: glidesys::LfbWriteMode::Rgb565,
            origin: glidesys::OriginLocation::UpperLeft,
            data: Vec::new(),
            width: 0,
            height: 0,
            stride: 0,
        }
    }
}

/// Resize a tightly packed BGRA8 image using nearest-neighbor sampling.
/// Glide exposes logical framebuffer coordinates even when the Vulkan
/// swapchain is configured at a larger presentation size.
fn scale_bgra_nearest(
    source: &[u8],
    source_width: u32,
    source_height: u32,
    destination_width: u32,
    destination_height: u32,
) -> Vec<u8> {
    if source_width == 0
        || source_height == 0
        || destination_width == 0
        || destination_height == 0
        || source.len() < (source_width * source_height * 4) as usize
    {
        return Vec::new();
    }

    if source_width == destination_width && source_height == destination_height {
        return source.to_vec();
    }

    let mut destination = vec![0u8; (destination_width * destination_height * 4) as usize];
    for y in 0..destination_height {
        let source_y = (y as u64 * source_height as u64 / destination_height as u64) as u32;
        for x in 0..destination_width {
            let source_x = (x as u64 * source_width as u64 / destination_width as u64) as u32;
            let source_offset = ((source_y * source_width + source_x) * 4) as usize;
            let destination_offset = ((y * destination_width + x) * 4) as usize;
            destination[destination_offset..destination_offset + 4]
                .copy_from_slice(&source[source_offset..source_offset + 4]);
        }
    }
    destination
}

fn lfb_to_bgra(
    width: u32,
    height: u32,
    stride: u32,
    write_mode: LfbWriteMode,
    origin: OriginLocation,
    src_data: &[u8],
) -> Vec<u8> {
    // Determine bytes per pixel based on write mode
    let bytes_per_pixel: u32 = match write_mode {
        glidesys::LfbWriteMode::Rgb565
        | glidesys::LfbWriteMode::Rgb555
        | glidesys::LfbWriteMode::Argb1555 => 2,
        glidesys::LfbWriteMode::Rgb888 => 3,
        glidesys::LfbWriteMode::Argb8888 => 4,
        _ => 2,
    };

    // Convert source data to BGRA8888 (matching swapchain format B8G8R8A8_UNORM)
    let mut bgra_data = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        let src_y = if origin == glidesys::OriginLocation::LowerLeft {
            height - 1 - y
        } else {
            y
        };

        for x in 0..width {
            let src_idx = (src_y * stride + x * bytes_per_pixel) as usize;
            let dst_idx = ((y * width + x) * 4) as usize;

            if dst_idx + 3 < bgra_data.len() {
                let (r, g, b, a) = match write_mode {
                    glidesys::LfbWriteMode::Rgb565 | glidesys::LfbWriteMode::Any => {
                        if src_idx + 1 < src_data.len() {
                            let rgb565 =
                                (src_data[src_idx] as u16) | ((src_data[src_idx + 1] as u16) << 8);
                            let r5 = ((rgb565 >> 11) & 0x1F) as u8;
                            let g6 = ((rgb565 >> 5) & 0x3F) as u8;
                            let b5 = (rgb565 & 0x1F) as u8;
                            (
                                (r5 << 3) | (r5 >> 2),
                                (g6 << 2) | (g6 >> 4),
                                (b5 << 3) | (b5 >> 2),
                                255u8,
                            )
                        } else {
                            (0, 0, 0, 255)
                        }
                    }
                    glidesys::LfbWriteMode::Rgb555 => {
                        if src_idx + 1 < src_data.len() {
                            let rgb555 =
                                (src_data[src_idx] as u16) | ((src_data[src_idx + 1] as u16) << 8);
                            let r5 = ((rgb555 >> 10) & 0x1F) as u8;
                            let g5 = ((rgb555 >> 5) & 0x1F) as u8;
                            let b5 = (rgb555 & 0x1F) as u8;
                            (
                                (r5 << 3) | (r5 >> 2),
                                (g5 << 3) | (g5 >> 2),
                                (b5 << 3) | (b5 >> 2),
                                255u8,
                            )
                        } else {
                            (0, 0, 0, 255)
                        }
                    }
                    glidesys::LfbWriteMode::Argb1555 => {
                        if src_idx + 1 < src_data.len() {
                            let argb1555 =
                                (src_data[src_idx] as u16) | ((src_data[src_idx + 1] as u16) << 8);
                            let a1 = ((argb1555 >> 15) & 0x1) as u8;
                            let r5 = ((argb1555 >> 10) & 0x1F) as u8;
                            let g5 = ((argb1555 >> 5) & 0x1F) as u8;
                            let b5 = (argb1555 & 0x1F) as u8;
                            (
                                (r5 << 3) | (r5 >> 2),
                                (g5 << 3) | (g5 >> 2),
                                (b5 << 3) | (b5 >> 2),
                                if a1 != 0 { 255 } else { 0 },
                            )
                        } else {
                            (0, 0, 0, 255)
                        }
                    }
                    glidesys::LfbWriteMode::Rgb888 => {
                        if src_idx + 2 < src_data.len() {
                            (
                                src_data[src_idx + 2],
                                src_data[src_idx + 1],
                                src_data[src_idx],
                                255u8,
                            )
                        } else {
                            (0, 0, 0, 255)
                        }
                    }
                    glidesys::LfbWriteMode::Argb8888 => {
                        if src_idx + 3 < src_data.len() {
                            (
                                src_data[src_idx + 2],
                                src_data[src_idx + 1],
                                src_data[src_idx],
                                src_data[src_idx + 3],
                            )
                        } else {
                            (0, 0, 0, 255)
                        }
                    }
                    _ => (0, 0, 0, 255),
                };

                // Write as BGRA (matching swapchain B8G8R8A8_UNORM)
                bgra_data[dst_idx] = b;
                bgra_data[dst_idx + 1] = g;
                bgra_data[dst_idx + 2] = r;
                bgra_data[dst_idx + 3] = a;
            }
        }
    }

    bgra_data
}

pub struct Gr {
    /// Wrapper configuration loaded from glidevk.toml
    config: config::Config,
    error_callback: unsafe extern "C" fn(*const c_char, FxBool),
    vulkan_instance: VulkanInstance,
    vulkan_app: Option<VulkanApp>,
    window: Option<Window>,
    surface: vk::SurfaceKHR,
    /// Game-owned Win32 HWND passed to grSstWinOpen (zero on other platforms).
    native_window_handle: FxU32,
    state: State,
    is_framebuffer_resized: bool,
    /// Vertex batch for collecting triangles between buffer_clear and buffer_swap
    vertex_batch: utility::vertex::VertexBatch,
    /// Texture memory for storing Glide textures
    texture_memory: [utility::texture::TextureMemory; GLIDE_NUM_TMU],
    /// LFB lock state
    lfb_lock_state: LfbLockState,
    /// FPS counter state
    fps_counter: FpsCounter,
    /// Texture addresses referenced this frame (for upload + eviction protection)
    frame_textures: std::collections::HashSet<u32>,
    continuation: Vec<utility::vertex::GpuVertex>,
    continuation_mode: PrimitiveType,
    continuation_odd: bool,
}

struct FpsCounter {
    last_time: std::time::Instant,
    frame_count: u32,
    display_fps: u32,
}

/// Maximum vertices per frame (enough for ~21k triangles)
const MAX_VERTICES: usize = 65536;

/// Maximum number of frames that can be rendered concurrently.
/// Each frame-in-flight gets its own synchronization objects to avoid
/// semaphore reuse while a previous frame is still being presented.
const MAX_FRAMES_IN_FLIGHT: usize = 2;

impl Gr {
    pub fn init(window: Option<Window>) -> Gr {
        // Load configuration from glidevk.toml (or use defaults)
        let config = config::Config::load();
        let texture_memory_bytes = config.texture.memory.bytes() as usize;
        log::info!(
            "GlideVK config: windowed={}, resizable={}, static_size={:?}, keep_aspect_ratio={}, clip_cursor={}, texture_memory={}MB",
            config.window.windowed,
            config.window.resizable,
            config.window.static_size,
            config.window.keep_aspect_ratio,
            config.window.clip_cursor,
            texture_memory_bytes / (1024 * 1024)
        );

        let vulkan_instance = VulkanInstance::new(cfg!(target_os = "linux"));
        Gr {
            config,
            error_callback,
            vulkan_instance,
            vulkan_app: None,
            window,
            surface: vk::SurfaceKHR::null(),
            native_window_handle: 0,
            state: Default::default(),
            is_framebuffer_resized: false,
            vertex_batch: utility::vertex::VertexBatch::new(MAX_VERTICES),
            texture_memory: [
                utility::texture::TextureMemory::new(texture_memory_bytes),
                utility::texture::TextureMemory::new(texture_memory_bytes),
            ],
            lfb_lock_state: LfbLockState::default(),
            fps_counter: FpsCounter {
                last_time: std::time::Instant::now(),
                frame_count: 0,
                display_fps: 0,
            },
            frame_textures: std::collections::HashSet::new(),
            continuation: Vec::new(),
            continuation_mode: PrimitiveType::Triangles,
            continuation_odd: false,
        }
    }

    /// Get a reference to the configuration
    pub fn config(&self) -> &config::Config {
        &self.config
    }

    /// Calculate the actual window size based on config and game resolution
    fn calculate_window_size(&self) -> (u32, u32) {
        let game_width = self.state.screen_width;
        let game_height = self.state.screen_height;

        // If static_size is configured, use it
        if self.config.window.static_size.is_set() {
            // Get desktop size for Desktop mode
            let desktop_size = self.get_desktop_size().unwrap_or((1920, 1080));

            if let Some((w, h)) = self.config.window.static_size.resolve(desktop_size) {
                return (w, h);
            }
        }

        // Default: use game's requested resolution
        (game_width, game_height)
    }

    /// Get the current desktop/monitor size
    fn get_desktop_size(&self) -> Option<(u32, u32)> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            if let Some(window) = &self.window {
                if let Some(monitor) = window.current_monitor() {
                    let size = monitor.size();
                    return Some((size.width, size.height));
                }
            }
        }
        #[cfg(target_os = "windows")]
        if let Some(info) =
            unsafe { utility::platforms::native_monitor_info(self.native_window_handle) }
        {
            return Some((
                (info.rcMonitor.right - info.rcMonitor.left) as u32,
                (info.rcMonitor.bottom - info.rcMonitor.top) as u32,
            ));
        }
        // Default fallback
        None
    }

    /// Calculate the viewport for letterboxing (maintains 4:3 aspect ratio if configured)
    fn calculate_viewport(&self) -> (u32, u32, u32, u32) {
        let game_width = self.state.screen_width;
        let game_height = self.state.screen_height;
        let window_width = self.state.window_width;
        let window_height = self.state.window_height;

        // If window matches game size or keep_aspect_ratio is disabled, use full window
        if !self.config.window.keep_aspect_ratio
            || (window_width == game_width && window_height == game_height)
        {
            return (0, 0, window_width, window_height);
        }

        // Calculate letterbox to maintain game's aspect ratio
        let game_aspect = game_width as f32 / game_height as f32;
        let window_aspect = window_width as f32 / window_height as f32;

        if window_aspect > game_aspect {
            // Window is wider than game - pillarbox (black bars on sides)
            let viewport_height = window_height;
            let viewport_width = (window_height as f32 * game_aspect) as u32;
            let viewport_x = (window_width - viewport_width) / 2;
            (viewport_x, 0, viewport_width, viewport_height)
        } else {
            // Window is taller than game - letterbox (black bars on top/bottom)
            let viewport_width = window_width;
            let viewport_height = (window_width as f32 / game_aspect) as u32;
            let viewport_y = (window_height - viewport_height) / 2;
            (0, viewport_y, viewport_width, viewport_height)
        }
    }

    #[cfg(target_os = "windows")]
    fn refresh_native_window_size(&mut self) {
        if self.native_window_handle == 0 {
            return;
        }

        let Some((width, height)) =
            (unsafe { utility::platforms::native_window_client_size(self.native_window_handle) })
        else {
            return;
        };
        if width == 0 || height == 0 {
            return;
        }

        let needs_recreate = self.is_framebuffer_resized
            || self.state.window_width != width
            || self.state.window_height != height;
        if !needs_recreate {
            return;
        }

        self.state.window_width = width;
        self.state.window_height = height;
        let (vp_x, vp_y, vp_w, vp_h) = self.calculate_viewport();
        self.state.viewport_x = vp_x;
        self.state.viewport_y = vp_y;
        self.state.viewport_width = vp_w;
        self.state.viewport_height = vp_h;

        if let Some(app) = &mut self.vulkan_app {
            match app.recreate_swapchain(
                &self.vulkan_instance.instance,
                &self.vulkan_instance.surface_loader,
                self.surface,
                width,
                height,
            ) {
                Ok(()) => self.is_framebuffer_resized = false,
                Err(error) => {
                    log::warn!(
                        "Deferring swapchain recreation for {}x{}: {:?}",
                        width,
                        height,
                        error
                    );
                    self.is_framebuffer_resized = true;
                }
            }
        }
    }

    // FX_ENTRY void FX_CALL grErrorSetCallback( GrErrorCallbackFnc_t fnc );
    pub fn error_set_callback(
        &mut self,
        func: extern "C" fn(string: *const c_char, fatal: FxBool),
    ) {
        self.error_callback = func;
    }

    // FX_ENTRY const char * FX_CALL grGetString( FxU32 pname );
    pub fn get_string(&mut self, sid: EnvironmentInfo) -> *const i8 {
        let str = match sid {
            EnvironmentInfo::Extension => EXTENSION,
            EnvironmentInfo::Hardware => HARDWARE,
            EnvironmentInfo::Renderer => RENDERER,
            EnvironmentInfo::Vendor => VENDOR,
            EnvironmentInfo::Version => VERSION,
        };
        return str.as_ptr() as *const i8;
    }

    // FX_ENTRY FxU32 FX_CALL grGet( FxU32 pname, FxU32 plength, FxI32 *params );
    pub fn get(
        &mut self,
        param_name: EnvironmentParameters,
        _param_len: FxU32,
        params: &mut FxI32,
    ) -> FxU32 {
        match param_name {
            EnvironmentParameters::NumBoards => *params = 1,
            EnvironmentParameters::MaxTextureSize => *params = 256,
            EnvironmentParameters::NumFb => *params = 1,
            _ => return 0,
        }
        return 1;
    }

    // FX_ENTRY FxBool FX_CALL grSelectContext( GrContext_t context );
    pub fn select_context(&mut self, _ctx: GrContext) -> FxBool {
        FxBool::True
    }

    // FX_ENTRY void FX_CALL grSstSelect(int which_sst)
    pub fn sst_select(&mut self, which_sst: i32) -> Option<()> {
        // check number of devices
        let instance = &self.vulkan_instance.instance;
        let physical_devices = unsafe {
            instance
                .enumerate_physical_devices()
                .expect("Failed to enumerate Physical Devices!")
        };
        let num_devices = physical_devices.len();
        log::debug!("{} devices (GPU) found with vulkan support.", num_devices);
        log::debug!("Found devices:");
        for &physical_device in physical_devices.iter() {
            let device_properties =
                unsafe { instance.get_physical_device_properties(physical_device) };
            let device_name = utility::tools::vk_to_string(&device_properties.device_name);
            log::debug!("\t{}", device_name);
        }
        if which_sst as usize >= num_devices {
            panic!("grSstSelect() must be lower than the number of devices.");
        }
        Some(())
    }

    pub fn sst_win_open(
        &mut self,
        hwnd: FxU32,
        res: ScreenResolution,
        refresh: ScreenRefresh,
        cformat: ColorFormat,
        origin: OriginLocation,
        num_buffers: i32,
        num_aux_buffers: i32,
    ) -> GrContext {
        #[cfg(target_os = "windows")]
        let hwnd = match (hwnd, self.window.as_ref()) {
            (0, Some(window)) => match window
                .window_handle()
                .expect("Window has no handle")
                .as_raw()
            {
                winit::raw_window_handle::RawWindowHandle::Win32(handle) => {
                    handle.hwnd.get() as u32
                }
                _ => panic!("Expected a Win32 window"),
            },
            _ => hwnd,
        };
        unsafe {
            log::debug!("Requested refresh: {:?}", refresh);
            self.state.origin = origin;
            self.native_window_handle = hwnd;

            // Get game's requested resolution
            let (game_width, game_height) = res.into();
            self.state.screen_width = game_width as u32;
            self.state.screen_height = game_height as u32;
            self.clip_window(0, 0, game_width as u32, game_height as u32);
            self.state.color_format = cformat;
            self.state.num_buffers = num_buffers;
            self.state.num_aux_buffers = num_aux_buffers;

            // Determine actual window size based on config
            let (window_width, window_height) = self.calculate_window_size();
            self.state.window_width = window_width;
            self.state.window_height = window_height;

            // Calculate viewport for letterboxing if keep_aspect_ratio is enabled
            let (vp_x, vp_y, vp_w, vp_h) = self.calculate_viewport();
            self.state.viewport_x = vp_x;
            self.state.viewport_y = vp_y;
            self.state.viewport_width = vp_w;
            self.state.viewport_height = vp_h;

            log::info!(
                "sst_win_open: game={}x{}, window={}x{}, viewport=({},{} {}x{})",
                self.state.screen_width,
                self.state.screen_height,
                window_width,
                window_height,
                vp_x,
                vp_y,
                vp_w,
                vp_h
            );

            let _entry = &self.vulkan_instance.entry;
            let instance = &self.vulkan_instance.instance;
            let surface_loader = &self.vulkan_instance.surface_loader;

            // Create a surface
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            {
                assert_eq!(hwnd, 0); // no hwnd for linux/macos app
                assert!(self.window.is_some()); // and window is already created
                let window = self.window.as_ref().unwrap();

                // Create a surface
                self.surface = ash_window::create_surface(
                    _entry,
                    instance,
                    window.display_handle().unwrap().as_raw(),
                    window.window_handle().unwrap().as_raw(),
                    None,
                )
                .unwrap();

                // Resize the window to configured size
                let new_size = winit::dpi::PhysicalSize::new(window_width, window_height);
                window.set_min_inner_size(Some(new_size));
                let _ = window.request_inner_size(new_size);

                // Center window if configured
                if self.config.window.centered {
                    if let Some(monitor) = window.current_monitor() {
                        let monitor_size = monitor.size();
                        let monitor_pos = monitor.position();
                        let x =
                            monitor_pos.x + ((monitor_size.width as i32 - window_width as i32) / 2);
                        let y = monitor_pos.y
                            + ((monitor_size.height as i32 - window_height as i32) / 2);
                        window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                    }
                }

                // Set resizable based on config
                window.set_resizable(self.config.window.resizable);
            }
            #[cfg(target_os = "windows")]
            {
                assert_ne!(hwnd, 0); // must have hwnd for windows app

                if self.config.window.windowed {
                    if let Some((actual_width, actual_height)) =
                        utility::platforms::configure_native_window(
                            hwnd,
                            window_width,
                            window_height,
                            self.config.window.resizable,
                            self.config.window.centered,
                        )
                    {
                        self.state.window_width = actual_width;
                        self.state.window_height = actual_height;
                        let (vp_x, vp_y, vp_w, vp_h) = self.calculate_viewport();
                        self.state.viewport_x = vp_x;
                        self.state.viewport_y = vp_y;
                        self.state.viewport_width = vp_w;
                        self.state.viewport_height = vp_h;
                        log::info!(
                            "Configured Win32 window: client={}x{}, resizable={}",
                            actual_width,
                            actual_height,
                            self.config.window.resizable
                        );
                    }
                }

                self.surface = utility::platforms::create_surface(
                    &self.vulkan_instance.entry,
                    &self.vulkan_instance.instance,
                    hwnd,
                )
                .expect("Failed to create surface.");
            }

            // Select physical device
            log::debug!("Selecting physical device...");
            let physical_device =
                vulkan::pick_physical_device(instance, surface_loader, &self.surface);
            log::debug!("Physical device selected, creating logical device...");
            // Select logical device
            let (logical_device, family_indices) = vulkan::create_logical_device(
                instance,
                surface_loader,
                &self.surface,
                physical_device,
            );
            log::debug!("Logical device created");
            // and get its queues
            let graphics_queue =
                logical_device.get_device_queue(family_indices.graphics_family.unwrap(), 0);
            let present_queue =
                logical_device.get_device_queue(family_indices.present_family.unwrap(), 0);
            log::debug!("Queues obtained");

            // Create a swapchain
            log::debug!("Querying swapchain support...");
            let swapchain_support =
                vulkan::query_swapchain_support(surface_loader, physical_device, &self.surface);
            log::debug!("Swapchain support queried");
            let surface_format = utility::choose_swapchain_format(&swapchain_support.formats);
            let present_mode =
                utility::choose_swapchain_present_mode(&swapchain_support.present_modes);
            // Use window dimensions for swapchain (may differ from game resolution due to static_size)
            let extent = utility::choose_swapchain_extent_size(
                &swapchain_support.capabilities,
                self.state.window_width,
                self.state.window_height,
            );

            let swapchain_loader = ash::khr::swapchain::Device::new(instance, &logical_device);
            // let swapchain_create_info = vulkan::get_swapchain_info(
            //     swapchain_support,
            //     &self.surface,
            //     surface_format,
            //     present_mode,
            //     &family_indices,
            //     width as u32,
            //     height as u32,
            // );

            let surface_capabilities = surface_loader
                .get_physical_device_surface_capabilities(physical_device, self.surface)
                .expect("Failed to query for surface capabilities.");

            let mut desired_image_count = surface_capabilities.min_image_count + 1;
            if surface_capabilities.max_image_count > 0
                && desired_image_count > surface_capabilities.max_image_count
            {
                desired_image_count = surface_capabilities.max_image_count;
            }
            let (image_sharing_mode, queue_family_indices) =
                if family_indices.graphics_family != family_indices.present_family {
                    (
                        vk::SharingMode::CONCURRENT,
                        vec![
                            family_indices.graphics_family.unwrap(),
                            family_indices.present_family.unwrap(),
                        ],
                    )
                } else {
                    (vk::SharingMode::EXCLUSIVE, vec![])
                };
            let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
                .surface(self.surface)
                .min_image_count(desired_image_count)
                .image_color_space(surface_format.color_space)
                .image_format(surface_format.format)
                .image_extent(extent)
                .image_usage(
                    vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::TRANSFER_SRC
                        | vk::ImageUsageFlags::TRANSFER_DST,
                )
                .image_sharing_mode(image_sharing_mode)
                .queue_family_indices(&queue_family_indices)
                .pre_transform(swapchain_support.capabilities.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(present_mode)
                .clipped(true)
                .old_swapchain(vk::SwapchainKHR::null())
                .image_array_layers(1);

            log::debug!("Creating swapchain...");
            let swapchain = swapchain_loader
                .create_swapchain(&swapchain_create_info, None)
                .expect("Failed to create Swapchain!");
            log::debug!("Swapchain created");
            let swapchain_images = swapchain_loader
                .get_swapchain_images(swapchain)
                .expect("Failed to get Swapchain Images.");
            log::debug!("Got {} swapchain images", swapchain_images.len());
            let swapchain_imageviews = utility::create_image_views(
                &logical_device,
                swapchain_create_info.image_format,
                &swapchain_images,
            );
            log::debug!("Created image views");

            // Create depth resources
            let depth_format = utility::find_depth_format(instance, physical_device);
            log::debug!("Depth format: {:?}", depth_format);
            let (depth_image, depth_image_memory, depth_image_view) =
                utility::create_depth_resources(
                    &logical_device,
                    instance,
                    physical_device,
                    extent,
                    depth_format,
                );
            log::debug!("Created depth resources");

            let render_pass: vk::RenderPass =
                utility::create_render_pass(&logical_device, surface_format.format, depth_format);
            log::debug!("Created render pass");

            // Create texture cache (needed for pipeline layout)
            let mut texture_cache = utility::texture::TextureCache::new(&logical_device);
            log::debug!("Created texture cache");

            let (graphics_pipelines, pipeline_layout) = utility::create_glide_graphics_pipelines(
                &logical_device,
                render_pass,
                extent,
                texture_cache.descriptor_set_layout(),
            );
            log::debug!("Created graphics pipeline");
            let swapchain_framebuffers = utility::create_framebuffers(
                &logical_device,
                render_pass,
                &swapchain_imageviews,
                depth_image_view,
                &extent,
            );
            log::debug!("Created framebuffers");
            // Create the command pool
            let family_indices =
                vulkan::find_queue_family(instance, physical_device, surface_loader, &self.surface);
            let command_pool = vulkan::create_command_pool(&logical_device, &family_indices);
            log::debug!("Created command pool");

            // Create command buffers
            let command_buffers = vulkan::create_command_buffers(
                &logical_device,
                command_pool,
                vk::Pipeline::null(), // unused param
                &swapchain_framebuffers,
                render_pass,
                extent,
            );
            log::debug!("Created command buffers");

            // Create synchronization objects:
            // - image_available + fences: per-frame-in-flight (indexed by current_frame)
            // - render_finished: per-swapchain-image (indexed by acquired image_index)
            //   because the swapchain holds the semaphore until the image is re-acquired
            let semaphore_info = vk::SemaphoreCreateInfo::default();
            let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
            let mut image_available_semaphores = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
            let mut in_flight_fences = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
            for _ in 0..MAX_FRAMES_IN_FLIGHT {
                image_available_semaphores.push(
                    logical_device
                        .create_semaphore(&semaphore_info, None)
                        .expect("Failed to create image available semaphore"),
                );
                in_flight_fences.push(
                    logical_device
                        .create_fence(&fence_info, None)
                        .expect("Failed to create in-flight fence"),
                );
            }
            let swapchain_image_count = swapchain_images.len();
            let mut render_finished_semaphores = Vec::with_capacity(swapchain_image_count);
            for _ in 0..swapchain_image_count {
                render_finished_semaphores.push(
                    logical_device
                        .create_semaphore(&semaphore_info, None)
                        .expect("Failed to create render finished semaphore"),
                );
            }
            log::debug!(
                "Created synchronization objects ({} frames in flight, {} swapchain images)",
                MAX_FRAMES_IN_FLIGHT,
                swapchain_image_count
            );

            // Create vertex buffers (one per frame-in-flight to avoid data races)
            let vertex_buffer_size = (MAX_VERTICES
                * std::mem::size_of::<utility::vertex::GpuVertex>())
                as vk::DeviceSize;
            let mut vertex_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
            for _ in 0..MAX_FRAMES_IN_FLIGHT {
                vertex_buffers.push(utility::vertex::VertexBuffer::new(
                    &logical_device,
                    instance,
                    physical_device,
                    vertex_buffer_size,
                    vk::BufferUsageFlags::VERTEX_BUFFER,
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                ));
            }
            log::debug!(
                "Created {} vertex buffers ({} bytes each)",
                MAX_FRAMES_IN_FLIGHT,
                vertex_buffer_size
            );

            // Create placeholder texture for the texture cache
            texture_cache.create_placeholder(
                &logical_device,
                instance,
                physical_device,
                command_pool,
                graphics_queue,
            );
            log::debug!("Created placeholder texture");

            self.vulkan_app = Some(VulkanApp {
                device: logical_device,
                framebuffer: None,
                state_pipelines: std::collections::HashMap::new(),
                graphics_queue,
                present_queue,
                swapchain_loader,
                swapchain,

                render_pass,
                pipeline_layout,
                graphics_pipelines,

                swapchain_format: swapchain_create_info.image_format,
                swapchain_extent: swapchain_create_info.image_extent,
                swapchain_images,
                swapchain_imageviews,
                swapchain_framebuffers,
                command_pool,
                command_buffers,

                image_available_semaphores,
                render_finished_semaphores,
                in_flight_fences,
                current_frame: 0,
                current_image_index: 0,
                last_presented_image_index: 0,

                vertex_buffers,
                physical_device,

                depth_image,
                depth_image_memory,
                depth_image_view,
                depth_format,

                texture_cache,
            });
            let app = self.vulkan_app.as_mut().unwrap();
            app.framebuffer = Some(utility::framebuffer::Framebuffer::new(
                app,
                instance,
                self.state.screen_width,
                self.state.screen_height,
            ));
            log::debug!("Vulkan app initialized successfully");
        }

        log::debug!("sst_win_open completed");
        return FxBool::True.0 as u32; // context
    }

    /*
    NAME
        grSstWinClose – close a graphics context
    C SPECIFICATION
        FxBool grSstWinClose( GrContext_t context )
    PARAMETERS
        context
            The graphics context to close.
    DESCRIPTION
        grSstWinClose closes a graphics context. It will fail, returning FXFALSE , if context is not a valid handle
        to a graphics context. Otherwise, it returns the state of Glide to the one following grGlideInit , so that
        grSstWinOpen can be called to open a new context.
    NOTES
        Glide 3.0 is the first release to require the context parameter for grSstWinClose.
    */
    ///
    /// Close graphics context and reset state to post-grGlideInit
    ///
    pub fn sst_win_close(&mut self, ctx: GrContext) -> FxBool {
        if ctx == 0 {
            return FxBool::False;
        }

        // Delete vulkan app (swapchain, device, etc.)
        self.vulkan_app = None;

        // Delete window (only for Linux/macOS where we manage the window)
        self.window = None;

        // Destroy surface if valid
        if self.surface != vk::SurfaceKHR::null() {
            unsafe {
                self.vulkan_instance
                    .surface_loader
                    .destroy_surface(self.surface, None);
            }
            self.surface = vk::SurfaceKHR::null();
        }

        // Reset state to post-grGlideInit (as per Glide spec)
        self.state = State::default();
        #[cfg(target_os = "windows")]
        unsafe {
            utility::platforms::update_cursor_clip(self.native_window_handle, false);
        }
        self.native_window_handle = 0;
        self.is_framebuffer_resized = false;

        // Clear texture memory
        for tmu in &mut self.texture_memory {
            tmu.clear();
        }

        // Clear vertex buffer
        self.vertex_batch.clear();
        self.frame_textures.clear();

        // Clear LFB state
        self.lfb_lock_state = LfbLockState::default();

        FxBool::True
    }

    /*
    NAME
        grClipWindow – set the size and location of the hardware clipping window
    C SPECIFICATION
        void grClipWindow(FxU32 minx, FxU32 miny, FxU32 maxx, FxU32 maxy )
    PARAMETERS
        minx The lower x screen coordinate of the clipping window.
        miny The lower y screen coordinate of the clipping window.
        maxx The upper x screen coordinate of the clipping window.
        maxy The upper y screen coordinate of the clipping window.
    DESCRIPTION
        grClipWindow specifies the hardware clipping window. Any pixels outside the clipping window are
        rejected. Values are inclusive for minimum x and y values and exclusive for maximum x and y values. The
        clipping window also specifies the area grBufferClear clears.
        At startup the default values for the clip window are the full size of the screen, e.g. (0,0,640,480) for
        640×480 mode and (0,0,800,600) for 800×600 mode. To disable clipping simply sets the size of the clip
        window to the screen size. The clipping window should not be used for general purpose primitive clipping;
        since clipped pixels are processed but discarded; proper geometric clipping should be done by the
        application for best performance. The clipping window should be used to prevent stray pixels that appear
        from imprecise geometric clipping. Note that if the pixel pipeline is disabled (see grLfbLock), clipping is
        not performed on linear frame buffer writes.
     */
    pub fn clip_window(&mut self, minx: FxU32, miny: FxU32, maxx: FxU32, maxy: FxU32) {
        self.state.clipwindowf_xmin = minx as f32;
        self.state.clipwindowf_xmax = maxx as f32;
        self.state.clipwindowf_ymin = miny as f32;
        self.state.clipwindowf_ymax = maxy as f32;
    }

    pub fn buffer_swap(&mut self, _interval: i32) -> Option<()> {
        #[cfg(target_os = "windows")]
        unsafe {
            utility::platforms::update_cursor_clip(
                self.native_window_handle,
                self.config.window.windowed && self.config.window.clip_cursor,
            );
        }
        self.finish();
        self.draw_fps_overlay();
        let app = self.vulkan_app.as_ref()?;
        app.framebuffer.as_ref()?.copy_front(app);
        self.present_front()
    }

    /*
    NAME
        grBufferClear – clear the buffers to the specified values
    C SPECIFICATION
        void grBufferClear( GrColor_t color, GrAlpha_t alpha, FxU32 depth )
    PARAMETERS
        colorThe color value used for clearing the draw buffer.
        alphaThe alpha value used for clearing the alpha buffer (ignored if alpha buffering is not
        enabled, i.e. a destination alpha is not specified in grAlphaBlendFunction).
        depthAn unsigned value used for clearing the depth buffer (ignored if depth buffering is
        not enabled).
    DESCRIPTION
        Clears the appropriate buffers with the given values. grClipWindow defines the area within the buffer to
        be cleared. Any buffers that are enabled are cleared by grBufferClear. For example, if depth buffering is
        enabled, the depth buffer will be cleared. If an application does not want a buffer to be cleared, then it
        should mask off writes to the buffer using grDepthMask and/or grColorMask as appropriate.
    */
    pub fn buffer_clear(&mut self, color: Color, _alpha: Alpha, depth: FxU32) -> Option<()> {
        self.finish();
        if self.vulkan_app.is_none() {
            return None;
        }
        let mut pipeline = self.pipeline_state();
        pipeline.depth_test = false;
        pipeline.depth_write = false;
        pipeline.blend = [1, 0, 1, 0];
        let mut vertices = Vec::new();
        // vkCmdClearAttachments has no per-channel mask. A solid quad handles masked color clears.
        if pipeline.color_mask != 0 && pipeline.color_mask != 15 {
            let (r, g, b, _) = color.to_rgba_f(self.state.color_format);
            push_quad(
                &mut vertices,
                0.0,
                0.0,
                self.state.screen_width as f32,
                self.state.screen_height as f32,
                [r, g, b, _alpha.to_f32()],
            );
        }
        let mut push = self.push_constants();
        push.fog_mode = 0;
        push.alpha_test = CmpFnc::Always as u32;
        self.render_vertices(
            &vertices,
            pipeline,
            push,
            self.clip_rect(),
            Some((color, _alpha, depth)),
        );
        Some(())
    }

    // ==================== Depth Buffer Functions ====================

    /// grDepthBufferMode - specify the depth buffering mode
    pub fn depth_buffer_mode(&mut self, mode: DepthBufferMode) {
        self.state.depth_buffer_mode = mode;
        log::debug!("Depth buffer mode set to: {:?}", mode);
    }

    /// grDepthBufferFunction - specify the depth buffer comparison function
    pub fn depth_buffer_function(&mut self, function: CmpFnc) {
        self.state.depth_buffer_function = function;
        log::debug!("Depth buffer function set to: {:?}", function);
    }

    /// grDepthMask - enable/disable writes to the depth buffer
    pub fn depth_mask(&mut self, mask: FxBool) {
        self.state.depth_mask = mask.is_true();
        log::debug!("Depth mask set to: {}", self.state.depth_mask);
    }

    // ==================== Color Mask Functions ====================

    /// grDitherMode - specify the dither mode
    pub fn dither_mode(&mut self, mode: DitherMode) {
        self.state.dither_mode = mode;
        log::trace!("Dither mode set to: {:?}", mode);
    }

    /// grColorMask - enable/disable writing to the color and alpha buffers
    pub fn color_mask(&mut self, rgb: FxBool, alpha: FxBool) {
        self.state.color_mask_rgb = rgb.is_true();
        self.state.color_mask_alpha = alpha.is_true();
        log::debug!(
            "Color mask set to RGB: {}, Alpha: {}",
            self.state.color_mask_rgb,
            self.state.color_mask_alpha
        );
    }

    // ==================== Alpha Blend Functions ====================

    /// grAlphaBlendFunction - specify the alpha blending function
    pub fn alpha_blend_function(
        &mut self,
        rgb_sf: AlphaBlendFnc,
        rgb_df: AlphaBlendFnc,
        alpha_sf: AlphaBlendFnc,
        alpha_df: AlphaBlendFnc,
    ) {
        self.state.alpha_blend_rgb_src = rgb_sf;
        self.state.alpha_blend_rgb_dst = rgb_df;
        self.state.alpha_blend_alpha_src = alpha_sf;
        self.state.alpha_blend_alpha_dst = alpha_df;
    }

    // ==================== Cull Mode Functions ====================

    /// grCullMode - set the cull mode
    pub fn cull_mode(&mut self, mode: CullMode) {
        self.state.cull_mode = mode;
        log::debug!("Cull mode set to: {:?}", mode);
    }

    // ==================== Render Buffer Functions ====================

    /// grRenderBuffer - select the buffer to render to
    pub fn render_buffer(&mut self, buffer: Buffer) {
        self.finish();
        self.state.render_buffer = buffer;
        log::debug!("Render buffer set to: {:?}", buffer);
    }

    /// grSstOrigin - set the Y origin for rendering
    ///
    /// This affects the coordinate system used for vertex Y coordinates.
    /// - UpperLeft: Y increases downward (default, matches screen coordinates)
    /// - LowerLeft: Y increases upward (OpenGL-style)
    pub fn sst_origin(&mut self, origin: OriginLocation) {
        self.state.origin = origin;
        log::trace!("SST origin set to: {:?}", origin);
    }

    // ==================== Constant Color Functions ====================

    /// grConstantColorValue - set the constant color
    /// Stored in state; written into vertex data at draw time (no draw call break)
    pub fn constant_color_value(&mut self, color: Color) {
        self.state.constant_color = color;
        log::trace!("Constant color set to: {:?}", color);
    }

    // ==================== Color/Alpha Combine Functions ====================

    /// grColorCombine - configure the color combine unit
    ///
    /// Determines how the final pixel color (RGB) is computed from local color,
    /// "other" color (texture or iterated), and a scale factor.
    pub fn color_combine(
        &mut self,
        function: CombineFunction,
        factor: CombineFactor,
        local: CombineLocal,
        other: CombineOther,
        invert: FxBool,
    ) {
        self.state.color_combine_function = function;
        self.state.color_combine_factor = factor;
        self.state.color_combine_local = local;
        self.state.color_combine_other = other;
        self.state.color_combine_invert = invert.is_true();
        log::trace!(
            "Color combine: function={:?}, factor={:?}, local={:?}, other={:?}, invert={:?}",
            function,
            factor,
            local,
            other,
            invert
        );
    }

    /// grAlphaCombine - configure the alpha combine unit
    pub fn alpha_combine(
        &mut self,
        function: CombineFunction,
        factor: CombineFactor,
        local: CombineLocal,
        other: CombineOther,
        invert: FxBool,
    ) {
        self.state.alpha_combine_function = function;
        self.state.alpha_combine_factor = factor;
        self.state.alpha_combine_local = local;
        self.state.alpha_combine_other = other;
        self.state.alpha_combine_invert = invert.is_true();
        log::trace!(
            "Alpha combine: function={:?}, factor={:?}, local={:?}, other={:?}, invert={:?}",
            function,
            factor,
            local,
            other,
            invert
        );
    }

    /// Pack current combine state into a u32 for per-vertex storage
    fn current_combine_flags(&self) -> u32 {
        let ccf = self.state.color_combine_function.0 as u32 & 0x1F;
        let acf = (self.state.alpha_combine_function.0 as u32 & 0x1F) << 5;
        let ccfr = (self.state.color_combine_factor as u32 & 0xF) << 10;
        let acfr = (self.state.alpha_combine_factor as u32 & 0xF) << 14;
        let cco = (self.state.color_combine_other as u32 & 0x3) << 18;
        let acl = (self.state.alpha_combine_local as u32 & 0x3) << 20;
        let ccl = (self.state.color_combine_local as u32 & 0x3) << 22;
        let aco = (self.state.alpha_combine_other as u32 & 0x3) << 24;
        let ck = if matches!(self.state.chromakey_mode, ChromakeyMode::Enable) {
            1u32 << 26
        } else {
            0
        };
        ccf | acf
            | ccfr
            | acfr
            | cco
            | acl
            | ccl
            | aco
            | ck
            | ((self.state.color_combine_invert as u32) << 27)
            | ((self.state.alpha_combine_invert as u32) << 28)
    }

    /// Get the current constant color as normalized RGBA floats
    fn current_constant_color(&self) -> [f32; 4] {
        let (r, g, b, a) = self.state.constant_color.to_rgba_f(self.state.color_format);
        [r, g, b, a]
    }

    // ==================== Fog Functions ====================

    /// grFogMode - set the fog mode
    pub fn fog_mode(&mut self, mode: FogMode) {
        self.state.fog_mode = mode;
        log::trace!("Fog mode set to: {:?}", mode);
    }

    /// grFogColorValue - set the fog color
    pub fn fog_color_value(&mut self, color: Color) {
        self.state.fog_color = color;
        log::trace!("Fog color set to: {:?}", color);
    }

    /// grFogTable - download a fog table
    pub fn fog_table(&mut self, table: *const u8) {
        if table.is_null() {
            return;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(table, self.state.fog_table.as_mut_ptr(), 64);
        }
        log::debug!("Fog table downloaded (64 entries)");
    }

    // ==================== Chromakey Functions ====================

    /// grChromakeyMode - enable/disable chroma-keying
    pub fn chromakey_mode(&mut self, mode: ChromakeyMode) {
        self.state.chromakey_mode = mode;
        log::trace!("Chromakey mode set to: {:?}", mode);
    }

    /// grChromakeyValue - set the chroma-key reference color
    pub fn chromakey_value(&mut self, color: Color) {
        self.state.chromakey_value = color;
        log::trace!("Chromakey value set to: {:?}", color);
    }

    // ==================== Alpha Test Functions ====================

    /// grAlphaTestFunction - set the alpha test function
    pub fn alpha_test_function(&mut self, function: CmpFnc) {
        self.state.alpha_test_function = function;
        log::trace!("Alpha test function set to: {:?}", function);
    }

    /// grAlphaTestReferenceValue - set the alpha test reference value
    pub fn alpha_test_reference_value(&mut self, value: Alpha) {
        self.state.alpha_test_reference = value;
        log::trace!("Alpha test reference set to: {:?}", value);
    }

    // ==================== Enable/Disable Functions ====================

    /// grEnable - enable a Glide feature
    pub fn enable(&mut self, mode: glidesys::EnableMode) {
        match mode {
            glidesys::EnableMode::AaOrdered => self.state.aa_ordered = true,
            glidesys::EnableMode::AllowMipmapDither => self.state.allow_mipmap_dither = true,
            glidesys::EnableMode::Passthru => self.state.passthru = true,
            glidesys::EnableMode::ShamelessPlug => self.state.shameless_plug = true,
            glidesys::EnableMode::VideoSmoothing => self.state.video_smoothing = true,
        }
        log::debug!("Enabled: {:?}", mode);
    }

    /// grDisable - disable a Glide feature
    pub fn disable(&mut self, mode: glidesys::EnableMode) {
        match mode {
            glidesys::EnableMode::AaOrdered => self.state.aa_ordered = false,
            glidesys::EnableMode::AllowMipmapDither => self.state.allow_mipmap_dither = false,
            glidesys::EnableMode::Passthru => self.state.passthru = false,
            glidesys::EnableMode::ShamelessPlug => self.state.shameless_plug = false,
            glidesys::EnableMode::VideoSmoothing => self.state.video_smoothing = false,
        }
        log::debug!("Disabled: {:?}", mode);
    }

    // ==================== Texture Functions ====================

    /// grTexSource - specify the current texture source for rendering
    pub fn tex_source(
        &mut self,
        tmu: ChipID,
        start_address: FxU32,
        even_odd: FxU32,
        info: &TexInfo,
    ) {
        let tmu_idx = tmu.0 as usize;
        if tmu_idx < GLIDE_NUM_TMU && info.is_valid() && even_odd & 3 != 0 {
            self.state.tex_source[tmu_idx] = start_address;
            log::trace!(
                "Texture source set for TMU{}: addr={:#x}, even_odd={}, format={:?}",
                tmu_idx,
                start_address,
                even_odd,
                info.format()
            );
        }
    }

    /// grTexFilterMode - set texture filtering mode
    pub fn tex_filter_mode(
        &mut self,
        tmu: ChipID,
        min_filter: TextureFilterMode,
        mag_filter: TextureFilterMode,
    ) {
        let tmu_idx = tmu.0 as usize;
        if tmu_idx < GLIDE_NUM_TMU {
            self.state.tex_filter_mode_min[tmu_idx] = min_filter;
            self.state.tex_filter_mode_mag[tmu_idx] = mag_filter;
            log::trace!(
                "Texture filter mode set for TMU{}: min={:?}, mag={:?}",
                tmu_idx,
                min_filter,
                mag_filter
            );
        }
    }

    /// grTexClampMode - set texture clamp mode
    pub fn tex_clamp_mode(
        &mut self,
        tmu: ChipID,
        s_clamp: TextureClampMode,
        t_clamp: TextureClampMode,
    ) {
        let tmu_idx = tmu.0 as usize;
        if tmu_idx < GLIDE_NUM_TMU {
            self.state.tex_clamp_mode_s[tmu_idx] = s_clamp;
            self.state.tex_clamp_mode_t[tmu_idx] = t_clamp;
            log::debug!(
                "Texture clamp mode set for TMU{}: s={:?}, t={:?}",
                tmu_idx,
                s_clamp,
                t_clamp
            );
        }
    }

    /// grTexMipMapMode - set mipmap mode
    pub fn tex_mipmap_mode(&mut self, tmu: ChipID, mode: MipMapMode, lod_blending: FxBool) {
        let tmu_idx = tmu.0 as usize;
        if tmu_idx < GLIDE_NUM_TMU {
            self.state.tex_mipmap_mode[tmu_idx] = mode;
            log::debug!(
                "Texture mipmap mode set for TMU{}: mode={:?}, lod_blending={:?}",
                tmu_idx,
                mode,
                lod_blending
            );
        }
    }

    /// grTexLodBiasValue - set LOD bias value
    pub fn tex_lod_bias_value(&mut self, tmu: ChipID, bias: f32) {
        let tmu_idx = tmu.0 as usize;
        if tmu_idx < GLIDE_NUM_TMU {
            self.state.tex_lod_bias[tmu_idx] = bias;
            log::debug!("Texture LOD bias set for TMU{}: {}", tmu_idx, bias);
        }
    }

    /// grTexCombine - set texture combine mode
    pub fn tex_combine(
        &mut self,
        tmu: ChipID,
        rgb_function: CombineFunction,
        rgb_factor: CombineFactor,
        alpha_function: CombineFunction,
        alpha_factor: CombineFactor,
        rgb_invert: FxBool,
        alpha_invert: FxBool,
    ) {
        let tmu_idx = tmu.0 as usize;
        if tmu_idx < GLIDE_NUM_TMU {
            self.state.tex_combine_function[tmu_idx] = rgb_function;
            self.state.tex_combine_factor[tmu_idx] = rgb_factor;
            self.state.tex_combine_alpha_function[tmu_idx] = alpha_function;
            self.state.tex_combine_alpha_factor[tmu_idx] = alpha_factor;
            self.state.tex_combine_rgb_invert[tmu_idx] = rgb_invert.is_true();
            self.state.tex_combine_alpha_invert[tmu_idx] = alpha_invert.is_true();
            log::debug!(
                "Texture combine set for TMU{}: rgb_func={:?}, rgb_factor={:?}, alpha_func={:?}, alpha_factor={:?}, rgb_invert={:?}, alpha_invert={:?}",
                tmu_idx, rgb_function, rgb_factor, alpha_function, alpha_factor, rgb_invert, alpha_invert
            );
        }
    }

    /// grTexMinAddress - return minimum valid texture start address
    /// Reference: docs/glide3x/cvg/glide3/src/ditex.c
    pub fn tex_min_address(&self, _tmu: ChipID) -> FxU32 {
        0
    }

    /// grTexMaxAddress - return maximum valid texture start address
    /// Reference: docs/glide3x/cvg/glide3/src/ditex.c
    /// Returns total_mem - 8 (last 8 bytes are reserved)
    pub fn tex_max_address(&self, tmu: ChipID) -> FxU32 {
        let tmu_idx = tmu.0 as usize;
        if tmu_idx < GLIDE_NUM_TMU {
            (self.texture_memory[tmu_idx].memory_available() as FxU32).saturating_sub(8)
        } else {
            0
        }
    }

    /// grTexTextureMemRequired - return bytes required for texture
    pub fn tex_texture_mem_required(&self, even_odd: FxU32, info: &TexInfo) -> FxU32 {
        info.get_size_mask(even_odd) as FxU32
    }

    /// Get texture memory size for a TMU (in bytes)
    /// GlideVK extension - not part of original Glide API
    pub fn get_texture_memory_size(&self, tmu: ChipID) -> usize {
        let tmu_idx = tmu.0 as usize;
        if tmu_idx < GLIDE_NUM_TMU {
            self.texture_memory[tmu_idx].memory_available()
        } else {
            0
        }
    }

    /// Set texture memory size for a TMU (in bytes)
    /// GlideVK extension - not part of original Glide API
    /// Common values: 2MB (0x200000), 4MB (0x400000), 8MB (0x800000)
    pub fn set_texture_memory_size(&mut self, tmu: ChipID, size: usize) {
        let tmu_idx = tmu.0 as usize;
        if tmu_idx < GLIDE_NUM_TMU {
            self.texture_memory[tmu_idx].set_max_memory(size);
            log::info!(
                "TMU{} texture memory set to {} bytes ({} MB)",
                tmu_idx,
                size,
                size / (1024 * 1024)
            );
        }
    }

    /// grVertexLayout - configure vertex component layout
    /// Reference: docs/glide3x/cvg/glide3/src/distrip.c
    ///
    /// # Arguments
    /// * `param` - Which vertex component to configure (GR_PARAM_*)
    /// * `offset` - Byte offset of this component within the vertex structure
    /// * `mode` - GR_PARAM_ENABLE or GR_PARAM_DISABLE
    pub fn vertex_layout(&mut self, param: FxU32, offset: FxU32, mode: FxU32) {
        log::debug!(
            "grVertexLayout: param={:#x}, offset={}, mode={}",
            param,
            offset,
            mode
        );

        let offset = offset as i32;
        let mode = if mode == 0 {
            VertexLayoutMode::Disable
        } else {
            VertexLayoutMode::Enable
        };

        let layout = &mut self.state.vertex_layout;

        match param {
            0x01 => {
                // GR_PARAM_XY - must be at offset 0
                if offset != 0 {
                    log::warn!(
                        "grVertexLayout: GR_PARAM_XY offset must be 0, got {}",
                        offset
                    );
                }
                layout.xy = VertexComponentInfo { offset, mode };
            }
            0x02 => {
                // GR_PARAM_Z
                layout.z = VertexComponentInfo { offset, mode };
            }
            0x03 => {
                // GR_PARAM_W
                layout.w = VertexComponentInfo { offset, mode };
            }
            0x04 => {
                // GR_PARAM_Q
                layout.q = VertexComponentInfo { offset, mode };
            }
            0x05 => {
                // GR_PARAM_FOG_EXT
                layout.fog = VertexComponentInfo { offset, mode };
            }
            0x10 => {
                // GR_PARAM_A
                layout.a = VertexComponentInfo { offset, mode };
                if mode == VertexLayoutMode::Enable {
                    layout.color_type = VertexColorType::Float;
                }
            }
            0x20 => {
                // GR_PARAM_RGB
                layout.rgb = VertexComponentInfo { offset, mode };
                if mode == VertexLayoutMode::Enable {
                    layout.color_type = VertexColorType::Float;
                }
            }
            0x30 => {
                // GR_PARAM_PARGB
                layout.pargb = VertexComponentInfo { offset, mode };
                if mode == VertexLayoutMode::Enable {
                    layout.color_type = VertexColorType::U8;
                }
            }
            0x40 => {
                // GR_PARAM_ST0
                layout.st0 = VertexComponentInfo { offset, mode };
            }
            0x41 => {
                // GR_PARAM_ST1
                layout.st1 = VertexComponentInfo { offset, mode };
            }
            0x50 => {
                // GR_PARAM_Q0
                layout.q0 = VertexComponentInfo { offset, mode };
            }
            0x51 => {
                // GR_PARAM_Q1
                layout.q1 = VertexComponentInfo { offset, mode };
            }
            _ => {
                log::warn!("grVertexLayout: unknown param {:#x}", param);
            }
        }
    }

    /// Get the current vertex layout
    pub fn get_vertex_layout(&self) -> &VertexLayout {
        &self.state.vertex_layout
    }

    /// Reset vertex layout to default state (for grReset with GR_VERTEX_PARAMETER)
    pub fn reset_vertex_layout(&mut self) {
        self.state.vertex_layout = VertexLayout::default();
    }

    /// grReset - reset statistics or vertex parameters
    /// Reference: docs/glide3x/h5/glide3/src/diget.c
    ///
    /// # Arguments
    /// * `what` - What to reset (GR_STATS_* or GR_VERTEX_PARAMETER)
    ///
    /// # Returns
    /// * FxBool::True if successful, FxBool::False otherwise
    pub fn reset(&mut self, what: FxU32) -> FxBool {
        // GR_STATS_POINTS = 0x1e
        // GR_STATS_LINES = 0x17
        // GR_STATS_PIXELS = 0x1d
        // GR_STATS_TRIANGLES = 0x21
        // GR_VERTEX_PARAMETER = 0x29
        match what {
            0x1e => {
                // GR_STATS_POINTS - reset points drawn counter
                // Stats not tracked in current implementation
                log::debug!("grReset: GR_STATS_POINTS");
                FxBool::True
            }
            0x17 => {
                // GR_STATS_LINES - reset lines drawn counter
                // Stats not tracked in current implementation
                log::debug!("grReset: GR_STATS_LINES");
                FxBool::True
            }
            0x1d => {
                // GR_STATS_PIXELS - reset pixel performance stats
                // Stats not tracked in current implementation
                log::debug!("grReset: GR_STATS_PIXELS");
                FxBool::True
            }
            0x21 => {
                // GR_STATS_TRIANGLES - reset triangle stats
                // Stats not tracked in current implementation
                log::debug!("grReset: GR_STATS_TRIANGLES");
                FxBool::True
            }
            0x29 => {
                // GR_VERTEX_PARAMETER - reset vertex layout to default
                log::debug!("grReset: GR_VERTEX_PARAMETER");
                self.reset_vertex_layout();
                FxBool::True
            }
            _ => {
                log::warn!("grReset: unknown parameter {:#x}", what);
                FxBool::False
            }
        }
    }

    pub fn tex_download_mipmap(
        &mut self,
        tmu: ChipID,
        address: FxU32,
        even_odd: FxU32,
        info: &TexInfo,
    ) {
        let data = unsafe { info.get_data() };
        let levels = info.levels();
        let Some(&(large_lod, _, _)) = levels.first() else {
            return;
        };
        let mut offset = 0;
        for (lod, width, height) in levels {
            let size = width * height * info.format().bytes_per_pixel();
            if offset + size > data.len() {
                return;
            }
            if even_odd & (1 << (lod as u32 & 1)) != 0 {
                self.tex_download_mipmap_level(
                    tmu,
                    address,
                    lod,
                    large_lod,
                    info.aspect_ratio(),
                    info.format(),
                    even_odd,
                    data[offset..].as_ptr().cast(),
                );
            }
            offset += size;
        }
    }

    pub fn tex_download_mipmap_level(
        &mut self,
        tmu: ChipID,
        address: FxU32,
        this_lod: LOD,
        large_lod: LOD,
        aspect: AspectRatio,
        format: TextureFormat,
        even_odd: FxU32,
        data: *const std::ffi::c_void,
    ) {
        let (_, height) = this_lod.to_texture_size(aspect);
        self.tex_download_mipmap_level_partial(
            tmu,
            address,
            this_lod,
            large_lod,
            aspect,
            format,
            even_odd,
            data,
            0,
            height as i32 - 1,
        );
    }

    pub fn tex_download_mipmap_level_partial(
        &mut self,
        tmu: ChipID,
        address: FxU32,
        this_lod: LOD,
        large_lod: LOD,
        aspect: AspectRatio,
        format: TextureFormat,
        even_odd: FxU32,
        data: *const std::ffi::c_void,
        start_row: i32,
        end_row: i32,
    ) -> FxBool {
        let tmu = tmu.0 as usize;
        let (width, height) = this_lod.to_texture_size(aspect);
        if tmu >= GLIDE_NUM_TMU
            || data.is_null()
            || this_lod as i32 > large_lod as i32
            || even_odd & (1 << (this_lod as u32 & 1)) == 0
            || start_row < 0
            || end_row < start_row
            || end_row as usize >= height
        {
            return FxBool::False;
        }
        self.finish();
        let (base_width, base_height) = large_lod.to_texture_size(aspect);
        let mut texture = self.texture_memory[tmu]
            .get_texture(address)
            .filter(|tex| {
                (tex.width, tex.height, tex.format as i32)
                    == (base_width as u32, base_height as u32, format as i32)
            })
            .cloned()
            .unwrap_or_else(|| {
                utility::texture::TextureData::new(
                    base_width as u32,
                    base_height as u32,
                    format,
                    vec![0; base_width * base_height * format.bytes_per_pixel()],
                )
            });
        let level = texture.level_mut(width as u32, height as u32);
        let row_bytes = width * format.bytes_per_pixel();
        let count = (end_row - start_row + 1) as usize * row_bytes;
        let offset = start_row as usize * row_bytes;
        let source = unsafe { std::slice::from_raw_parts(data as *const u8, count) };
        level.data[offset..offset + count].copy_from_slice(source);
        self.texture_memory[tmu].store_texture(address, texture);
        if let Some(app) = &mut self.vulkan_app {
            app.texture_cache
                .invalidate_texture(&app.device, address | ((tmu as u32) << 31));
        }
        FxBool::True
    }

    /// grTexDownloadTable - download texture color table (palette or NCC)
    /// Always downloads to TMU0 per Glide3 API spec
    pub fn tex_download_table(
        &mut self,
        table_type: TextureTableType,
        data: *const std::ffi::c_void,
    ) {
        if data.is_null() {
            log::warn!("grTexDownloadTable called with null data pointer");
            return;
        }

        match table_type {
            TextureTableType::Palette | TextureTableType::Palette6666Ext => {
                let raw = unsafe { std::slice::from_raw_parts(data as *const u8, 256 * 4) };
                let palette = if matches!(table_type, TextureTableType::Palette6666Ext) {
                    utility::texture::PaletteTable::from_argb6666_data(raw)
                } else {
                    utility::texture::PaletteTable::from_data(raw)
                };
                let old = self.texture_memory[0].get_palette(0);
                if old.map_or(true, |old| *old != palette) {
                    self.finish();
                    self.state.palette_generation = self.state.palette_generation.wrapping_add(1);
                    self.texture_memory[0].store_palette(0, palette);
                }
            }
            TextureTableType::Ncc0 | TextureTableType::Ncc1 => {
                self.finish();
                let raw = unsafe { std::slice::from_raw_parts(data as *const u8, 64) };
                self.texture_memory[0].store_ncc(table_type as usize, raw);
                self.state.palette_generation = self.state.palette_generation.wrapping_add(1);
            }
        }

        // Store current table type in state
        self.state.tex_table_type = table_type;
    }

    // ==================== Drawing Functions ====================

    fn gpu_vertex(&self, v: &Vertex, fog: f32) -> utility::vertex::GpuVertex {
        let mut gpu = utility::vertex::GpuVertex::from_glide_vertex(v);
        gpu.depth = match self.state.depth_buffer_mode {
            DepthBufferMode::ZBuffer | DepthBufferMode::ZBufferCompareToBias => {
                (v.ooz / 65535.0).clamp(0.0, 1.0)
            }
            // Normalized W depth preserves near/far clears; emulate the 3Dfx
            // 16-bit floating encoding if a title needs intermediate W clear values.
            DepthBufferMode::WBuffer | DepthBufferMode::WBufferCompareToBias => {
                1.0 - v.oow.clamp(0.0, 1.0)
            }
            _ => 0.0,
        };
        if api::current_coordinate_space() == CoordinateSpaceMode::Clip as u32 {
            gpu.position = [
                (v.x * v.oow + 1.0) * self.state.screen_width as f32 * 0.5,
                (v.y * v.oow + 1.0) * self.state.screen_height as f32 * 0.5,
            ];
            gpu.depth = ((v.ooz * v.oow + 1.0) * 0.5).clamp(0.0, 1.0);
            gpu.tex_coord[0] *= v.oow;
            gpu.tex_coord[1] *= v.oow;
            gpu.tex_q = v.oow;
            gpu.tex_coord1[0] *= v.oow;
            gpu.tex_coord1[1] *= v.oow;
            gpu.tex_q1 = v.oow;
        }
        if self.state.origin == OriginLocation::LowerLeft {
            gpu.position[1] = self.state.screen_height as f32 - gpu.position[1];
        }
        gpu.constant_color = self.current_constant_color();
        gpu.combine_flags = self.current_combine_flags();
        gpu.texture_index = self.state.tex_source[0];
        gpu.texture_index1 = if self.state.tex_source[1] == u32::MAX {
            u32::MAX
        } else {
            self.state.tex_source[1] | (1 << 31)
        };
        gpu.fog = if self.state.fog_mode.0 & 255 == 3 {
            v.ooz / 65535.0
        } else {
            fog
        };
        gpu
    }

    pub fn draw_triangle(&mut self, a: &Vertex, b: &Vertex, c: &Vertex) {
        self.track_textures();
        let vertices = [
            self.gpu_vertex(a, a.oow),
            self.gpu_vertex(b, b.oow),
            self.gpu_vertex(c, c.oow),
        ];
        self.draw_primitives_gpu(PrimitiveType::Triangles, &vertices, self.state.cull_mode);
    }

    pub fn draw_point(&mut self, point: &Vertex) {
        self.track_textures();
        self.draw_point_gpu(self.gpu_vertex(point, point.oow));
    }

    pub fn draw_line(&mut self, a: &Vertex, b: &Vertex) {
        self.track_textures();
        self.draw_line_gpu(self.gpu_vertex(a, a.oow), self.gpu_vertex(b, b.oow));
    }

    /// # Safety
    /// Each pointer must contain the fields enabled in the current vertex layout.
    pub unsafe fn draw_vertex_array(
        &mut self,
        mode: PrimitiveType,
        count: FxU32,
        pointers: *const *const Vertex,
    ) {
        if pointers.is_null() || count == 0 {
            return;
        }
        self.track_textures();
        let size = self.state.vertex_layout.vertex_size();
        let pointers = std::slice::from_raw_parts(pointers, count as usize);
        if pointers.iter().any(|p| p.is_null()) {
            return;
        }
        let vertices: Vec<_> = pointers
            .iter()
            .map(|&ptr| {
                let (vertex, fog) = self
                    .state
                    .vertex_layout
                    .decode(std::slice::from_raw_parts(ptr as *const u8, size));
                self.gpu_vertex(&vertex, fog)
            })
            .collect();
        self.draw_primitives_gpu(mode, &vertices, self.state.cull_mode);
    }

    pub fn draw_vertex_array_contiguous(
        &mut self,
        mode: PrimitiveType,
        count: FxU32,
        pointers: *const Vertex,
        stride: FxU32,
    ) {
        let size = self.state.vertex_layout.vertex_size();
        if pointers.is_null() || count == 0 || (stride as usize) < size {
            return;
        }
        if (count as usize)
            .checked_mul(stride as usize)
            .is_none_or(|size| size > isize::MAX as usize)
        {
            return;
        }
        self.track_textures();
        let vertices: Vec<_> = (0..count as usize)
            .map(|i| unsafe {
                let data = std::slice::from_raw_parts(
                    (pointers as *const u8).add(i * stride as usize),
                    size,
                );
                let (vertex, fog) = self.state.vertex_layout.decode(data);
                self.gpu_vertex(&vertex, fog)
            })
            .collect();
        self.draw_primitives_gpu(mode, &vertices, self.state.cull_mode);
    }

    /// Internal helper to draw primitives from GPU vertices
    fn draw_primitives_gpu(
        &mut self,
        mode: PrimitiveType,
        vertices: &[utility::vertex::GpuVertex],
        cull_mode: CullMode,
    ) {
        let continuing = matches!(
            mode,
            PrimitiveType::TriangleStripContinue | PrimitiveType::TriangleFanContinue
        );
        let base_mode = match mode {
            PrimitiveType::TriangleStripContinue => PrimitiveType::TriangleStrip,
            PrimitiveType::TriangleFanContinue => PrimitiveType::TriangleFan,
            other => other,
        };
        if continuing && self.continuation_mode != base_mode {
            return;
        }
        let previous = if continuing {
            self.continuation.len()
        } else {
            0
        };
        let odd = continuing && previous == 2 && self.continuation_odd;
        let mut joined = if continuing {
            self.continuation.clone()
        } else {
            Vec::new()
        };
        joined.extend_from_slice(vertices);
        if matches!(
            base_mode,
            PrimitiveType::TriangleStrip | PrimitiveType::TriangleFan
        ) {
            self.continuation_odd =
                (continuing && self.continuation_odd) ^ (vertices.len() % 2 != 0);
            self.continuation = if base_mode == PrimitiveType::TriangleFan && joined.len() > 1 {
                vec![joined[0], *joined.last().unwrap()]
            } else {
                joined[joined.len().saturating_sub(2)..].to_vec()
            };
            self.continuation_mode = base_mode;
        } else {
            self.continuation.clear();
        }
        let vertices = joined.as_slice();
        let mode = base_mode;
        match mode {
            PrimitiveType::Points => {
                for v in vertices {
                    self.draw_point_gpu(*v);
                }
            }
            PrimitiveType::Lines => {
                for chunk in vertices.chunks(2) {
                    if chunk.len() == 2 {
                        self.draw_line_gpu(chunk[0], chunk[1]);
                    }
                }
            }
            PrimitiveType::LineStrip => {
                for window in vertices.windows(2) {
                    self.draw_line_gpu(window[0], window[1]);
                }
            }
            PrimitiveType::Triangles => {
                for chunk in vertices.chunks(3) {
                    if chunk.len() == 3 {
                        self.draw_triangle_gpu(chunk[0], chunk[1], chunk[2], cull_mode);
                    }
                }
            }
            PrimitiveType::TriangleFan => {
                if vertices.len() >= 3 {
                    let first = vertices[0];
                    for i in previous.saturating_sub(1).max(1)..vertices.len() - 1 {
                        self.draw_triangle_gpu(first, vertices[i], vertices[i + 1], cull_mode);
                    }
                }
            }
            PrimitiveType::TriangleStrip => {
                for i in previous.saturating_sub(2)..vertices.len().saturating_sub(2) {
                    if (i % 2 != 0) == odd {
                        self.draw_triangle_gpu(
                            vertices[i],
                            vertices[i + 1],
                            vertices[i + 2],
                            cull_mode,
                        );
                    } else {
                        self.draw_triangle_gpu(
                            vertices[i + 1],
                            vertices[i],
                            vertices[i + 2],
                            cull_mode,
                        );
                    }
                }
            }
            PrimitiveType::Polygon => {
                if vertices.len() >= 3 {
                    let first = vertices[0];
                    for i in previous.saturating_sub(1).max(1)..vertices.len() - 1 {
                        self.draw_triangle_gpu(first, vertices[i], vertices[i + 1], cull_mode);
                    }
                }
            }
            _ => log::warn!("Unsupported primitive value {}", mode.0),
        }
    }

    /// Draw a triangle from GPU vertices with culling
    fn draw_triangle_gpu(
        &mut self,
        v0: utility::vertex::GpuVertex,
        v1: utility::vertex::GpuVertex,
        v2: utility::vertex::GpuVertex,
        cull_mode: CullMode,
    ) {
        // Check culling
        if cull_mode != CullMode::Disable {
            let (ax, ay) = (v0.position[0], v0.position[1]);
            let (bx, by) = (v1.position[0], v1.position[1]);
            let (cx, cy) = (v2.position[0], v2.position[1]);
            let area = (bx - ax) * (cy - ay) - (cx - ax) * (by - ay);
            match cull_mode {
                CullMode::Negative if area < 0.0 => return,
                CullMode::Positive if area > 0.0 => return,
                _ => {}
            }
        }

        if !self.vertex_batch.push_triangle(v0, v1, v2) {
            log::warn!("Vertex batch full, triangle dropped");
        }
    }

    /// Draw a point as a small quad from GPU vertex
    fn draw_point_gpu(&mut self, v: utility::vertex::GpuVertex) {
        const POINT_SIZE: f32 = 0.5;
        let x = v.position[0];
        let y = v.position[1];

        let v0 = utility::vertex::GpuVertex {
            position: [x - POINT_SIZE, y - POINT_SIZE],
            ..v
        };
        let v1 = utility::vertex::GpuVertex {
            position: [x + POINT_SIZE, y - POINT_SIZE],
            ..v
        };
        let v2 = utility::vertex::GpuVertex {
            position: [x + POINT_SIZE, y + POINT_SIZE],
            ..v
        };
        let v3 = utility::vertex::GpuVertex {
            position: [x - POINT_SIZE, y + POINT_SIZE],
            ..v
        };

        if !self.vertex_batch.push_triangle(v0, v1, v2) {
            log::warn!("Vertex batch full, point dropped");
            return;
        }
        if !self.vertex_batch.push_triangle(v0, v2, v3) {
            log::warn!("Vertex batch full, point dropped");
        }
    }

    /// Draw a line as a thin quad from GPU vertices
    fn draw_line_gpu(&mut self, v1: utility::vertex::GpuVertex, v2: utility::vertex::GpuVertex) {
        const LINE_WIDTH: f32 = 0.5;

        let dx = v2.position[0] - v1.position[0];
        let dy = v2.position[1] - v1.position[1];
        let len = (dx * dx + dy * dy).sqrt();

        if len < 0.001 {
            return;
        }

        let nx = -dy / len * LINE_WIDTH;
        let ny = dx / len * LINE_WIDTH;

        let p0 = utility::vertex::GpuVertex {
            position: [v1.position[0] + nx, v1.position[1] + ny],
            ..v1
        };
        let p1 = utility::vertex::GpuVertex {
            position: [v1.position[0] - nx, v1.position[1] - ny],
            ..v1
        };
        let p2 = utility::vertex::GpuVertex {
            position: [v2.position[0] - nx, v2.position[1] - ny],
            ..v2
        };
        let p3 = utility::vertex::GpuVertex {
            position: [v2.position[0] + nx, v2.position[1] + ny],
            ..v2
        };

        if !self.vertex_batch.push_triangle(p0, p1, p2) {
            log::warn!("Vertex batch full, line dropped");
            return;
        }
        if !self.vertex_batch.push_triangle(p0, p2, p3) {
            log::warn!("Vertex batch full, line dropped");
        }
    }

    /// Get a reference to the window
    pub fn window_ref(&self) -> Option<&Window> {
        self.window.as_ref()
    }

    /// Get the actual screen/framebuffer width in pixels
    pub fn sst_screen_width(&self) -> u32 {
        self.vulkan_app
            .as_ref()
            .map(|app| app.swapchain_extent.width)
            .unwrap_or(0)
    }

    /// Get the actual screen/framebuffer height in pixels
    pub fn sst_screen_height(&self) -> u32 {
        self.vulkan_app
            .as_ref()
            .map(|app| app.swapchain_extent.height)
            .unwrap_or(0)
    }

    fn track_textures(&mut self) {
        let addresses = [
            self.state.tex_source[0],
            if self.state.tex_source[1] == u32::MAX {
                u32::MAX
            } else {
                self.state.tex_source[1] | (1 << 31)
            },
        ];
        let extra = addresses
            .iter()
            .filter(|&&a| a != u32::MAX && !self.frame_textures.contains(&a))
            .count();
        if self.frame_textures.len() + extra > utility::texture::DISPLAY_SLOT as usize - 1 {
            self.finish();
        }
        self.frame_textures
            .extend(addresses.into_iter().filter(|&a| a != u32::MAX));
        self.vertex_batch.set_state(utility::vertex::DrawState {
            pipeline: self.pipeline_state(),
            push: self.push_constants(),
            clip: self.clip_rect(),
        });
    }

    fn read_buffer(&mut self, buffer: Buffer) -> Option<(u32, u32, Vec<u8>)> {
        if !matches!(buffer, Buffer::Front | Buffer::Back) {
            return None;
        }
        self.finish();
        let app = self.vulkan_app.as_ref()?;
        let fb = app.framebuffer.as_ref()?;
        Some((
            fb.extent.width,
            fb.extent.height,
            fb.read(
                app,
                &self.vulkan_instance.instance,
                if buffer == Buffer::Front { 0 } else { 1 },
            ),
        ))
    }

    /// Read the displayed logical framebuffer, before display gamma and scaling.
    pub fn lfb_read_region(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        self.read_buffer(Buffer::Front)
    }

    /// Copy a pixel rectangle from the frame buffer to user memory as RGB565
    ///
    /// # Arguments
    /// * `_src_buffer` - Source buffer (currently ignored, uses current swapchain image)
    /// * `src_x`, `src_y` - Source coordinates (y origin is upper left)
    /// * `src_width`, `src_height` - Size of rectangle to copy
    /// * `dst_stride` - Stride in bytes of destination buffer
    /// * `dst_data` - Pointer to destination buffer (RGB565 format)
    ///
    /// # Returns
    /// FxBool::True on success, FxBool::False on failure
    pub fn lfb_read_region_rgb565(
        &mut self,
        src_buffer: glidesys::Buffer,
        src_x: FxU32,
        src_y: FxU32,
        src_width: FxU32,
        src_height: FxU32,
        dst_stride: FxU32,
        dst_data: *mut std::ffi::c_void,
    ) -> glidesys::FxBool {
        if dst_data.is_null() || dst_stride < src_width.saturating_mul(2) {
            return glidesys::FxBool::False;
        }

        // Read entire framebuffer
        if let Some((swapchain_width, swapchain_height, pixels)) = self.read_buffer(src_buffer) {
            let fb_width = if self.state.screen_width > 0 {
                self.state.screen_width
            } else {
                swapchain_width
            };
            let fb_height = if self.state.screen_height > 0 {
                self.state.screen_height
            } else {
                swapchain_height
            };
            let pixels = scale_bgra_nearest(
                &pixels,
                swapchain_width,
                swapchain_height,
                fb_width,
                fb_height,
            );

            // Validate bounds
            if src_x
                .checked_add(src_width)
                .is_none_or(|right| right > fb_width)
                || src_y
                    .checked_add(src_height)
                    .is_none_or(|bottom| bottom > fb_height)
                || pixels.is_empty()
            {
                return glidesys::FxBool::False;
            }

            // Copy requested region to destination buffer
            // Source is BGRA (4 bytes per pixel), destination is RGB565 (2 bytes per pixel)
            let dst = dst_data as *mut u8;
            for row in 0..src_height {
                let src_row_start = ((src_y + row) * fb_width + src_x) as usize * 4;
                let dst_row_start = (row * dst_stride) as usize;

                for col in 0..src_width {
                    let src_idx = src_row_start + (col as usize) * 4;
                    let dst_idx = dst_row_start + (col as usize) * 2;

                    // Convert BGRA to RGB565
                    let b = pixels[src_idx] as u16;
                    let g = pixels[src_idx + 1] as u16;
                    let r = pixels[src_idx + 2] as u16;

                    // RGB565: RRRRR GGGGGG BBBBB
                    let rgb565: u16 = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);

                    unsafe {
                        let dst_ptr = dst.add(dst_idx) as *mut u16;
                        dst_ptr.write_unaligned(rgb565);
                    }
                }
            }
            glidesys::FxBool::True
        } else {
            glidesys::FxBool::False
        }
    }

    /// Lock the linear frame buffer for direct access
    ///
    /// # Arguments
    /// * `lock_type` - Read or write lock
    /// * `buffer` - Which buffer to lock (front/back)
    /// * `write_mode` - Pixel format for write operations
    /// * `origin` - Y origin location
    /// * `pixel_pipeline` - Whether to use pixel pipeline (ignored)
    /// * `info` - Output: filled with lock information
    ///
    /// # Returns
    /// FxBool::True if lock succeeded, FxBool::False otherwise
    pub fn lfb_lock(
        &mut self,
        lock_type: glidesys::LfbLock,
        buffer: glidesys::Buffer,
        write_mode: glidesys::LfbWriteMode,
        origin: glidesys::OriginLocation,
        _pixel_pipeline: FxBool,
        info: &mut glidesys::LfbInfo,
    ) -> FxBool {
        let lock_type = LfbLock(lock_type.0 & !LfbLock::NoIdle.0);
        if !matches!(lock_type, LfbLock::ReadOnly | LfbLock::WriteOnly)
            || !matches!(buffer, Buffer::Front | Buffer::Back)
        {
            return FxBool::False;
        }
        // Check if already locked
        if self.lfb_lock_state.is_locked {
            log::warn!("LFB already locked");
            return FxBool::False;
        }

        let app = match self.vulkan_app.as_ref() {
            Some(app) => app,
            None => return FxBool::False,
        };

        // Glide applications always address the logical game framebuffer.
        // Presentation scaling is handled when writing to or reading from the
        // Vulkan swapchain.
        let (width, height) = if self.state.screen_width > 0 && self.state.screen_height > 0 {
            (self.state.screen_width, self.state.screen_height)
        } else {
            (app.swapchain_extent.width, app.swapchain_extent.height)
        };

        let write_mode = if write_mode == LfbWriteMode::Any {
            LfbWriteMode::Rgb565
        } else {
            write_mode
        };
        let bytes_per_pixel = match write_mode {
            LfbWriteMode::Rgb565 | LfbWriteMode::Rgb555 | LfbWriteMode::Argb1555 => 2,
            LfbWriteMode::Rgb888 => 3,
            LfbWriteMode::Argb8888 => 4,
            _ => return FxBool::False,
        };
        if !matches!(
            origin,
            OriginLocation::UpperLeft | OriginLocation::LowerLeft
        ) {
            return FxBool::False;
        }
        let stride = width * bytes_per_pixel;
        let Some((_, _, pixels)) = self.read_buffer(buffer) else {
            return FxBool::False;
        };
        // Preload write locks too: applications may update only part of the locked image.
        // Full readback/convert preserves untouched pixels; track dirty rows if LFB profiling warrants it.
        let mut data = vec![0; (stride * height) as usize];
        for y in 0..height {
            let source_y = if origin == OriginLocation::LowerLeft {
                height - 1 - y
            } else {
                y
            };
            for x in 0..width {
                let source = ((source_y * width + x) * 4) as usize;
                let dest = (y * stride + x * bytes_per_pixel) as usize;
                let [b, g, r, a]: [u8; 4] = pixels[source..source + 4].try_into().unwrap();
                if bytes_per_pixel == 2 {
                    let value = match write_mode {
                        LfbWriteMode::Rgb565 => {
                            ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3)
                        }
                        _ => {
                            ((r as u16 >> 3) << 10)
                                | ((g as u16 >> 3) << 5)
                                | (b as u16 >> 3)
                                | if write_mode == LfbWriteMode::Argb1555 && a >= 128 {
                                    0x8000
                                } else {
                                    0
                                }
                        }
                    };
                    data[dest..dest + 2].copy_from_slice(&value.to_le_bytes());
                } else {
                    data[dest..dest + bytes_per_pixel as usize]
                        .copy_from_slice(&[b, g, r, a][..bytes_per_pixel as usize]);
                }
            }
        }

        // Store lock state
        self.lfb_lock_state.is_locked = true;
        self.lfb_lock_state.lock_type = lock_type;
        self.lfb_lock_state.buffer = buffer;
        self.lfb_lock_state.write_mode = write_mode;
        self.lfb_lock_state.origin = origin;
        self.lfb_lock_state.width = width;
        self.lfb_lock_state.height = height;
        self.lfb_lock_state.stride = stride;
        self.lfb_lock_state.data = data;

        // Fill info structure
        info.size = std::mem::size_of::<glidesys::LfbInfo>() as i32;
        info.lfb_ptr = self.lfb_lock_state.data.as_mut_ptr() as *mut std::ffi::c_void;
        info.stride_in_bytes = stride;
        info.write_mode = write_mode;
        info.origin = origin;

        FxBool::True
    }

    /// Unlock the linear frame buffer
    ///
    /// # Arguments
    /// * `lock_type` - The lock type used when locking
    /// * `buffer` - The buffer that was locked
    ///
    /// # Returns
    /// FxBool::True if unlock succeeded
    pub fn lfb_unlock(&mut self, lock_type: glidesys::LfbLock, buffer: glidesys::Buffer) -> FxBool {
        let lock_type = LfbLock(lock_type.0 & !LfbLock::NoIdle.0);
        if !self.lfb_lock_state.is_locked {
            return FxBool::False;
        }

        if self.lfb_lock_state.lock_type != lock_type || self.lfb_lock_state.buffer != buffer {
            return FxBool::False;
        }

        // For write locks, upload data directly to swapchain image and present
        if lock_type == glidesys::LfbLock::WriteOnly {
            self.lfb_upload_to_framebuffer();
        }

        // Clear lock state
        self.lfb_lock_state.is_locked = false;
        self.lfb_lock_state.data.clear();

        FxBool::True
    }

    pub fn lfb_write_region(
        &mut self,
        dst_buffer: Buffer,
        dst_x: FxU32,
        dst_y: FxU32,
        src_format: LfbWriteMode,
        width: FxU32,
        height: FxU32,
        _pixel_pipeline: FxBool,
        stride: i32,
        data: *const std::ffi::c_void,
    ) -> FxBool {
        let bpp = match src_format {
            LfbWriteMode::Rgb565 | LfbWriteMode::Rgb555 | LfbWriteMode::Argb1555 => 2,
            LfbWriteMode::Rgb888 => 3,
            LfbWriteMode::Argb8888 => 4,
            _ => return FxBool::False,
        };
        if self.lfb_lock_state.is_locked
            || data.is_null()
            || width == 0
            || height == 0
            || stride <= 0
            || !matches!(dst_buffer, Buffer::Front | Buffer::Back)
            || width.checked_mul(bpp).is_none_or(|row| row > stride as u32)
            || dst_x
                .checked_add(width)
                .is_none_or(|right| right > self.state.screen_width)
            || dst_y
                .checked_add(height)
                .is_none_or(|bottom| bottom > self.state.screen_height)
        {
            return FxBool::False;
        }
        let Some(size) = (height as usize - 1)
            .checked_mul(stride as usize)
            .and_then(|n| n.checked_add(width as usize * bpp as usize))
        else {
            return FxBool::False;
        };
        if size > isize::MAX as usize {
            return FxBool::False;
        }
        self.finish();
        let pixels = lfb_to_bgra(
            width,
            height,
            stride as u32,
            src_format,
            OriginLocation::UpperLeft,
            unsafe { std::slice::from_raw_parts(data as *const u8, size) },
        );
        let Some(app) = &self.vulkan_app else {
            return FxBool::False;
        };
        let Some(fb) = &app.framebuffer else {
            return FxBool::False;
        };
        fb.write_rect(
            app,
            &self.vulkan_instance.instance,
            if dst_buffer == Buffer::Front { 0 } else { 1 },
            &pixels,
            dst_x,
            dst_y,
            width,
            height,
        );
        if dst_buffer == Buffer::Front && self.present_front().is_none() {
            return FxBool::False;
        }
        FxBool::True
    }

    fn lfb_upload_to_framebuffer(&mut self) {
        self.finish();
        let width = self.lfb_lock_state.width;
        let height = self.lfb_lock_state.height;
        let stride = self.lfb_lock_state.stride;
        let write_mode = self.lfb_lock_state.write_mode;
        let origin = self.lfb_lock_state.origin;
        let src_data = &self.lfb_lock_state.data;
        let bgra_data = lfb_to_bgra(width, height, stride, write_mode, origin, src_data);
        if let Some(app) = &self.vulkan_app {
            if let Some(fb) = &app.framebuffer {
                fb.write(
                    app,
                    &self.vulkan_instance.instance,
                    if self.lfb_lock_state.buffer == Buffer::Front {
                        0
                    } else {
                        1
                    },
                    &bgra_data,
                );
            }
        }
        if self.lfb_lock_state.buffer == Buffer::Front {
            self.present_front();
        }
    }
}

#[cfg(target_os = "linux")]
impl WindowInteraction for Gr {
    fn draw_frame(&mut self, _delta_time: f32) {
        // todo!()
    }

    fn recreate_swapchain(&mut self) {
        todo!()
    }

    fn cleanup_swapchain(&self) {
        if let Some(app) = &self.vulkan_app {
            app.cleanup_swapchain()
        }
    }

    fn wait_device_idle(&self) {
        unsafe {
            if let Some(app) = &self.vulkan_app {
                app.get_device()
                    .device_wait_idle()
                    .expect("Failed to wait device idle!")
            }
        };
    }

    fn resize_framebuffer(&mut self) {
        self.is_framebuffer_resized = true;
    }

    fn window_ref(&self) -> Option<&Window> {
        self.window.as_ref()
    }
}

impl Drop for Gr {
    fn drop(&mut self) {
        log::debug!("Dropping Gr");

        #[cfg(target_os = "windows")]
        unsafe {
            utility::platforms::update_cursor_clip(self.native_window_handle, false);
        }

        // Clean up Vulkan resources if they exist
        self.vulkan_app = None;
        self.window = None;

        // Destroy surface if valid
        if self.surface != vk::SurfaceKHR::null() {
            unsafe {
                self.vulkan_instance
                    .surface_loader
                    .destroy_surface(self.surface, None);
            }
            self.surface = vk::SurfaceKHR::null();
        }

        // VulkanInstance handles its own cleanup in its Drop impl
        log::debug!("Gr dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a Gr instance with Vulkan for testing.
    /// Note: Requires MoltenVK ICD to be configured on macOS:
    /// export VK_ICD_FILENAMES=/opt/homebrew/opt/molten-vk/etc/vulkan/icd.d/MoltenVK_icd.json
    fn create_test_gr() -> Gr {
        Gr::init(None)
    }

    #[test]
    fn continuation_batches_and_clip_coordinates() {
        use utility::vertex::GpuVertex;
        let mut gr = create_test_gr();
        let vertices: Vec<_> = (0..6)
            .map(|i| GpuVertex {
                position: [i as f32, 0.0],
                ..Default::default()
            })
            .collect();
        gr.draw_primitives_gpu(
            PrimitiveType::TriangleStrip,
            &vertices[..3],
            CullMode::Disable,
        );
        for v in &vertices[3..] {
            gr.draw_primitives_gpu(
                PrimitiveType::TriangleStripContinue,
                &[*v],
                CullMode::Disable,
            );
            assert_eq!(gr.continuation.len(), 2);
        }
        let xs: Vec<_> = gr
            .vertex_batch
            .vertices
            .iter()
            .map(|v| v.position[0] as u32)
            .collect();
        assert_eq!(xs, [0, 1, 2, 2, 1, 3, 2, 3, 4, 4, 3, 5]);
        gr.vertex_batch.clear();
        gr.draw_primitives_gpu(
            PrimitiveType::TriangleFan,
            &vertices[..2],
            CullMode::Disable,
        );
        for v in &vertices[2..] {
            gr.draw_primitives_gpu(PrimitiveType::TriangleFanContinue, &[*v], CullMode::Disable);
        }
        let xs: Vec<_> = gr
            .vertex_batch
            .vertices
            .iter()
            .map(|v| v.position[0] as u32)
            .collect();
        assert_eq!(xs, [0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 5]);
        gr.vertex_batch.clear();
        for _ in 0..22000 {
            gr.draw_primitives_gpu(PrimitiveType::Triangles, &vertices[..3], CullMode::Disable);
        }
        assert_eq!(
            gr.vertex_batch.vertices.len(),
            66000,
            "batch limit must not drop triangles"
        );
        gr.vertex_batch.clear();
        gr.state.screen_width = 800;
        gr.state.screen_height = 600;
        gr.clip_window(0, 0, 800, 600);
        gr.draw_triangle(
            &Vertex::new(0.0, 0.0),
            &Vertex::new(1.0, 0.0),
            &Vertex::new(0.0, 1.0),
        );
        gr.clip_window(10, 20, 30, 40);
        gr.alpha_blend_function(
            AlphaBlendFnc::SrcAlpha,
            AlphaBlendFnc::One,
            AlphaBlendFnc::One,
            AlphaBlendFnc::Zero,
        );
        gr.draw_triangle(
            &Vertex::new(0.0, 0.0),
            &Vertex::new(1.0, 0.0),
            &Vertex::new(0.0, 1.0),
        );
        gr.vertex_batch.finalize();
        assert_eq!(
            gr.vertex_batch.draw_calls.len(),
            2,
            "state setters retain a single batch"
        );
        assert_eq!(
            gr.vertex_batch.draw_calls[0]
                .state
                .unwrap()
                .clip
                .extent
                .width,
            800
        );
        assert_eq!(
            gr.vertex_batch.draw_calls[1]
                .state
                .unwrap()
                .clip
                .extent
                .width,
            20
        );
        api::coordinate_space(CoordinateSpaceMode::Clip);
        let mut v = Vertex::new(-2.0, 2.0);
        v.oow = 0.5;
        v.tmuvtx[0].sow = 128.0;
        let gpu = gr.gpu_vertex(&v, 0.5);
        api::coordinate_space(CoordinateSpaceMode::Window);
        assert_eq!(gpu.position, [0.0, 600.0]);
        assert_eq!((gpu.tex_coord[0], gpu.tex_q), (64.0, 0.5));
    }

    // ==================== Gr::init Tests ====================

    #[test]
    fn test_gr_init_creates_vulkan_instance() {
        let gr = create_test_gr();
        // If we get here without panic, Vulkan instance was created successfully
        assert!(gr.vulkan_app.is_none()); // No window, so no VulkanApp yet
        assert!(gr.window.is_none());
    }

    #[test]
    fn test_gr_init_default_state() {
        let gr = create_test_gr();
        assert_eq!(gr.state.screen_width, 0);
        assert_eq!(gr.state.screen_height, 0);
        assert_eq!(gr.state.num_buffers, 0);
    }

    // ==================== grSstSelect Tests ====================

    #[test]
    fn test_sst_select_device_zero() {
        let mut gr = create_test_gr();
        let result = gr.sst_select(0);
        assert!(result.is_some());
    }

    #[test]
    #[should_panic(expected = "grSstSelect() must be lower than the number of devices")]
    fn test_sst_select_invalid_device() {
        let mut gr = create_test_gr();
        gr.sst_select(999); // Should panic - invalid device index
    }

    // ==================== grSelectContext Tests ====================

    #[test]
    fn test_select_context() {
        let mut gr = create_test_gr();
        let result = gr.select_context(1);
        assert!(matches!(result, FxBool::True));
    }

    // ==================== grClipWindow Tests ====================

    #[test]
    fn test_clip_window() {
        let mut gr = create_test_gr();
        gr.clip_window(10, 20, 630, 460);
        assert_eq!(gr.state.clipwindowf_xmin, 10.0);
        assert_eq!(gr.state.clipwindowf_ymin, 20.0);
        assert_eq!(gr.state.clipwindowf_xmax, 630.0);
        assert_eq!(gr.state.clipwindowf_ymax, 460.0);
    }

    #[test]
    fn test_clip_window_fullscreen() {
        let mut gr = create_test_gr();
        gr.clip_window(0, 0, 800, 600);
        assert_eq!(gr.state.clipwindowf_xmin, 0.0);
        assert_eq!(gr.state.clipwindowf_ymin, 0.0);
        assert_eq!(gr.state.clipwindowf_xmax, 800.0);
        assert_eq!(gr.state.clipwindowf_ymax, 600.0);
    }

    // ==================== Vertex Batch Tests ====================

    #[test]
    fn test_vertex_batch_initial_empty() {
        let gr = create_test_gr();
        assert!(gr.vertex_batch.is_empty());
        assert_eq!(gr.vertex_batch.vertex_count(), 0);
    }

    // ==================== Color Conversion Tests ====================

    #[test]
    fn test_color_to_rgba_argb() {
        let color = Color(0xFF804020); // ARGB: A=FF, R=80, G=40, B=20
        let (r, g, b, a) = color.to_rgba(ColorFormat::Argb);
        assert_eq!(r, 0x80);
        assert_eq!(g, 0x40);
        assert_eq!(b, 0x20);
        assert_eq!(a, 0xFF);
    }

    #[test]
    fn test_color_to_rgba_abgr() {
        let color = Color(0xFF204080); // ABGR: A=FF, B=20, G=40, R=80
        let (r, g, b, a) = color.to_rgba(ColorFormat::Abgr);
        assert_eq!(r, 0x80);
        assert_eq!(g, 0x40);
        assert_eq!(b, 0x20);
        assert_eq!(a, 0xFF);
    }

    #[test]
    fn test_color_to_rgba_f() {
        let color = Color(0xFF000000); // ARGB: A=FF, R=00, G=00, B=00
        let (r, g, b, a) = color.to_rgba_f(ColorFormat::Argb);
        assert_eq!(r, 0.0);
        assert_eq!(g, 0.0);
        assert_eq!(b, 0.0);
        assert!((a - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_color_to_rgba_f_white() {
        let color = Color(0xFFFFFFFF); // ARGB: all max
        let (r, g, b, a) = color.to_rgba_f(ColorFormat::Argb);
        assert!((r - 1.0).abs() < 0.01);
        assert!((g - 1.0).abs() < 0.01);
        assert!((b - 1.0).abs() < 0.01);
        assert!((a - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_scale_bgra_nearest_preserves_pixel_blocks() {
        let source = vec![1, 0, 0, 255, 2, 0, 0, 255, 3, 0, 0, 255, 4, 0, 0, 255];
        let scaled = scale_bgra_nearest(&source, 2, 2, 4, 4);

        let pixel = |x: usize, y: usize| scaled[(y * 4 + x) * 4];
        assert_eq!(pixel(0, 0), 1);
        assert_eq!(pixel(1, 1), 1);
        assert_eq!(pixel(2, 0), 2);
        assert_eq!(pixel(0, 2), 3);
        assert_eq!(pixel(3, 3), 4);
    }

    // ==================== Alpha Tests ====================

    #[test]
    fn test_alpha_to_f32() {
        let alpha = Alpha(255);
        assert!((alpha.to_f32() - 1.0).abs() < 0.01);

        let alpha_half = Alpha(128);
        assert!((alpha_half.to_f32() - 0.5).abs() < 0.02);

        let alpha_zero = Alpha(0);
        assert_eq!(alpha_zero.to_f32(), 0.0);
    }

    // ==================== Depth Buffer Tests ====================

    #[test]
    fn test_depth_buffer_mode() {
        let mut gr = create_test_gr();
        gr.depth_buffer_mode(DepthBufferMode::ZBuffer);
        assert!(matches!(
            gr.state.depth_buffer_mode,
            DepthBufferMode::ZBuffer
        ));
    }

    #[test]
    fn test_depth_buffer_function() {
        let mut gr = create_test_gr();
        gr.depth_buffer_function(CmpFnc::Less);
        assert!(matches!(gr.state.depth_buffer_function, CmpFnc::Less));
    }

    #[test]
    fn test_depth_mask() {
        let mut gr = create_test_gr();
        gr.depth_mask(FxBool::False);
        assert!(!gr.state.depth_mask);
        gr.depth_mask(FxBool::True);
        assert!(gr.state.depth_mask);
    }

    // ==================== Color Mask Tests ====================

    #[test]
    fn test_color_mask() {
        let mut gr = create_test_gr();
        gr.color_mask(FxBool::False, FxBool::True);
        assert!(!gr.state.color_mask_rgb);
        assert!(gr.state.color_mask_alpha);
    }

    // ==================== Alpha Blend Tests ====================

    #[test]
    fn test_alpha_blend_function() {
        let mut gr = create_test_gr();
        gr.alpha_blend_function(
            AlphaBlendFnc::SrcAlpha,
            AlphaBlendFnc::OneMinusSrcAlpha,
            AlphaBlendFnc::One,
            AlphaBlendFnc::Zero,
        );
        assert!(matches!(
            gr.state.alpha_blend_rgb_src,
            AlphaBlendFnc::SrcAlpha
        ));
        assert!(matches!(
            gr.state.alpha_blend_rgb_dst,
            AlphaBlendFnc::OneMinusSrcAlpha
        ));
    }

    // ==================== Cull Mode Tests ====================

    #[test]
    fn test_cull_mode() {
        let mut gr = create_test_gr();
        gr.cull_mode(CullMode::Negative);
        assert_eq!(gr.state.cull_mode, CullMode::Negative);
        gr.cull_mode(CullMode::Positive);
        assert_eq!(gr.state.cull_mode, CullMode::Positive);
    }

    // ==================== Render Buffer Tests ====================

    #[test]
    fn test_render_buffer() {
        let mut gr = create_test_gr();
        gr.render_buffer(Buffer::Front);
        assert!(matches!(gr.state.render_buffer, Buffer::Front));
    }

    // ==================== Constant Color Tests ====================

    #[test]
    fn test_constant_color_value() {
        let mut gr = create_test_gr();
        gr.constant_color_value(Color(0x12345678));
        assert_eq!(gr.state.constant_color.0, 0x12345678);
    }

    // ==================== Fog Tests ====================

    #[test]
    fn test_fog_mode() {
        let mut gr = create_test_gr();
        gr.fog_mode(FogMode::WithTable);
        assert!(matches!(gr.state.fog_mode, FogMode::WithTable));
    }

    #[test]
    fn test_fog_color_value() {
        let mut gr = create_test_gr();
        gr.fog_color_value(Color(0xAABBCCDD));
        assert_eq!(gr.state.fog_color.0, 0xAABBCCDD);
    }

    // ==================== Chromakey Tests ====================

    #[test]
    fn test_chromakey_mode() {
        let mut gr = create_test_gr();
        gr.chromakey_mode(ChromakeyMode::Enable);
        assert!(matches!(gr.state.chromakey_mode, ChromakeyMode::Enable));
    }

    #[test]
    fn test_chromakey_value() {
        let mut gr = create_test_gr();
        gr.chromakey_value(Color(0xFF00FF00));
        assert_eq!(gr.state.chromakey_value.0, 0xFF00FF00);
    }

    // ==================== Alpha Test Tests ====================

    #[test]
    fn test_alpha_test_function() {
        let mut gr = create_test_gr();
        gr.alpha_test_function(CmpFnc::Greater);
        assert!(matches!(gr.state.alpha_test_function, CmpFnc::Greater));
    }

    #[test]
    fn test_alpha_test_reference_value() {
        let mut gr = create_test_gr();
        gr.alpha_test_reference_value(Alpha(128));
        assert_eq!(gr.state.alpha_test_reference.0, 128);
    }

    // ==================== Draw Triangle Tests ====================

    #[test]
    fn test_draw_triangle_adds_to_batch() {
        let mut gr = create_test_gr();
        let v0 = Vertex::new(0.0, 0.0);
        let v1 = Vertex::new(100.0, 0.0);
        let v2 = Vertex::new(50.0, 100.0);
        gr.draw_triangle(&v0, &v1, &v2);
        assert_eq!(gr.vertex_batch.vertex_count(), 3);
    }

    #[test]
    fn test_draw_triangle_with_cull_negative() {
        let mut gr = create_test_gr();
        gr.cull_mode(CullMode::Negative);
        // Counter-clockwise triangle (negative area)
        let v0 = Vertex::new(0.0, 0.0);
        let v1 = Vertex::new(0.0, 100.0);
        let v2 = Vertex::new(100.0, 0.0);
        gr.draw_triangle(&v0, &v1, &v2);
        // Should be culled
        assert_eq!(gr.vertex_batch.vertex_count(), 0);
    }

    #[test]
    fn test_draw_triangle_with_cull_positive() {
        let mut gr = create_test_gr();
        gr.cull_mode(CullMode::Positive);
        // Clockwise triangle (positive area)
        let v0 = Vertex::new(0.0, 0.0);
        let v1 = Vertex::new(100.0, 0.0);
        let v2 = Vertex::new(50.0, 100.0);
        gr.draw_triangle(&v0, &v1, &v2);
        // Should be culled
        assert_eq!(gr.vertex_batch.vertex_count(), 0);
    }

    // ==================== Texture Tests ====================

    #[test]
    fn test_tex_filter_mode() {
        let mut gr = create_test_gr();
        gr.tex_filter_mode(
            ChipID::Tmu0,
            TextureFilterMode::Bilinear,
            TextureFilterMode::Bilinear,
        );
        assert!(matches!(
            gr.state.tex_filter_mode_min[0],
            TextureFilterMode::Bilinear
        ));
        assert!(matches!(
            gr.state.tex_filter_mode_mag[0],
            TextureFilterMode::Bilinear
        ));
    }

    #[test]
    fn test_tex_clamp_mode() {
        let mut gr = create_test_gr();
        gr.tex_clamp_mode(
            ChipID::Tmu0,
            TextureClampMode::Clamp,
            TextureClampMode::Wrap,
        );
        assert!(matches!(
            gr.state.tex_clamp_mode_s[0],
            TextureClampMode::Clamp
        ));
        assert!(matches!(
            gr.state.tex_clamp_mode_t[0],
            TextureClampMode::Wrap
        ));
    }

    #[test]
    fn test_tex_mipmap_mode() {
        let mut gr = create_test_gr();
        gr.tex_mipmap_mode(ChipID::Tmu0, MipMapMode::Nearest, FxBool::True);
        assert!(matches!(gr.state.tex_mipmap_mode[0], MipMapMode::Nearest));
    }

    #[test]
    fn test_tex_lod_bias_value() {
        let mut gr = create_test_gr();
        gr.tex_lod_bias_value(ChipID::Tmu0, -1.5);
        assert!((gr.state.tex_lod_bias[0] - (-1.5)).abs() < 0.001);
    }

    #[test]
    fn test_tex_combine() {
        let mut gr = create_test_gr();
        gr.tex_combine(
            ChipID::Tmu0,
            CombineFunction::Local,
            CombineFactor::One,
            CombineFunction::Local,
            CombineFactor::One,
            FxBool::False,
            FxBool::False,
        );
        assert!(matches!(
            gr.state.tex_combine_function[0],
            CombineFunction::Local
        ));
        assert!(matches!(gr.state.tex_combine_factor[0], CombineFactor::One));
    }

    #[test]
    fn test_tex_min_address() {
        let gr = create_test_gr();
        assert_eq!(gr.tex_min_address(ChipID::Tmu0), 0);
    }

    #[test]
    fn test_tex_max_address() {
        let gr = create_test_gr();
        // Should return configured texture memory - 8 (last 8 bytes reserved)
        // Default config is 8MB per TMU
        let expected = gr.config().texture.memory.bytes() - 8;
        assert_eq!(gr.tex_max_address(ChipID::Tmu0), expected);
    }

    #[test]
    fn test_palette_generation_only_changes_for_new_palette_data() {
        let mut gr = create_test_gr();
        let mut palette = [0u32; 256];
        palette[1] = 0xFF12_3456;

        gr.tex_download_table(TextureTableType::Palette, palette.as_ptr().cast());
        assert_eq!(gr.state.palette_generation, 1);

        gr.tex_download_table(TextureTableType::Palette, palette.as_ptr().cast());
        assert_eq!(gr.state.palette_generation, 1);

        palette[1] = 0xFF65_4321;
        gr.tex_download_table(TextureTableType::Palette, palette.as_ptr().cast());
        assert_eq!(gr.state.palette_generation, 2);
    }

    #[test]
    fn test_palette_6666_download_quantizes_argb_input() {
        let mut gr = create_test_gr();
        let mut palette = [0u32; 256];
        palette[0] = 0xFF80_4020;

        gr.tex_download_table(TextureTableType::Palette6666Ext, palette.as_ptr().cast());

        let stored = gr.texture_memory[0].get_palette(0).unwrap();
        assert_eq!(stored.entries[0], 0xFF82_4120);
        assert_eq!(gr.state.palette_generation, 1);
    }

    #[test]
    fn test_color_combine() {
        let mut gr = create_test_gr();
        gr.color_combine(
            CombineFunction::Local,
            CombineFactor::One,
            CombineLocal::Constant,
            CombineOther::Iterated,
            FxBool::True,
        );
        assert!(matches!(
            gr.state.color_combine_function,
            CombineFunction::Local
        ));
        assert!(matches!(gr.state.color_combine_factor, CombineFactor::One));
        assert!(matches!(
            gr.state.color_combine_local,
            CombineLocal::Constant
        ));
        assert!(matches!(
            gr.state.color_combine_other,
            CombineOther::Iterated
        ));
        assert!(gr.state.color_combine_invert);
    }

    #[test]
    fn test_alpha_combine() {
        let mut gr = create_test_gr();
        gr.alpha_combine(
            CombineFunction::ScaleOther,
            CombineFactor::Local,
            CombineLocal::Iterated,
            CombineOther::Texture,
            FxBool::False,
        );
        assert!(matches!(
            gr.state.alpha_combine_function,
            CombineFunction::ScaleOther
        ));
        assert!(matches!(
            gr.state.alpha_combine_factor,
            CombineFactor::Local
        ));
        assert!(matches!(
            gr.state.alpha_combine_local,
            CombineLocal::Iterated
        ));
        assert!(matches!(
            gr.state.alpha_combine_other,
            CombineOther::Texture
        ));
        assert!(!gr.state.alpha_combine_invert);
    }

    // ==================== grVertexLayout Tests ====================

    // GR_PARAM constants for tests
    const GR_PARAM_XY: u32 = 0x01;
    const GR_PARAM_Z: u32 = 0x02;
    const GR_PARAM_W: u32 = 0x03;
    const GR_PARAM_Q: u32 = 0x04;
    const GR_PARAM_A: u32 = 0x10;
    const GR_PARAM_RGB: u32 = 0x20;
    const GR_PARAM_PARGB: u32 = 0x30;
    const GR_PARAM_ST0: u32 = 0x40;
    const GR_PARAM_ST1: u32 = 0x41;
    const GR_PARAM_Q0: u32 = 0x50;
    const GR_PARAM_Q1: u32 = 0x51;

    const GR_PARAM_DISABLE: u32 = 0x00;
    const GR_PARAM_ENABLE: u32 = 0x01;

    #[test]
    fn test_vertex_layout_xy() {
        let mut gr = create_test_gr();
        gr.vertex_layout(GR_PARAM_XY, 0, GR_PARAM_ENABLE);

        let layout = gr.get_vertex_layout();
        assert_eq!(layout.xy.offset, 0);
        assert_eq!(layout.xy.mode, VertexLayoutMode::Enable);
    }

    #[test]
    fn test_vertex_layout_z() {
        let mut gr = create_test_gr();
        gr.vertex_layout(GR_PARAM_Z, 8, GR_PARAM_ENABLE);

        let layout = gr.get_vertex_layout();
        assert_eq!(layout.z.offset, 8);
        assert_eq!(layout.z.mode, VertexLayoutMode::Enable);
    }

    #[test]
    fn test_vertex_layout_w() {
        let mut gr = create_test_gr();
        gr.vertex_layout(GR_PARAM_W, 12, GR_PARAM_ENABLE);

        let layout = gr.get_vertex_layout();
        assert_eq!(layout.w.offset, 12);
        assert_eq!(layout.w.mode, VertexLayoutMode::Enable);
    }

    #[test]
    fn test_vertex_layout_rgb_sets_float_color_type() {
        let mut gr = create_test_gr();
        gr.vertex_layout(GR_PARAM_RGB, 16, GR_PARAM_ENABLE);

        let layout = gr.get_vertex_layout();
        assert_eq!(layout.rgb.offset, 16);
        assert_eq!(layout.rgb.mode, VertexLayoutMode::Enable);
        assert_eq!(layout.color_type, VertexColorType::Float);
    }

    #[test]
    fn test_vertex_layout_pargb_sets_u8_color_type() {
        let mut gr = create_test_gr();
        gr.vertex_layout(GR_PARAM_PARGB, 16, GR_PARAM_ENABLE);

        let layout = gr.get_vertex_layout();
        assert_eq!(layout.pargb.offset, 16);
        assert_eq!(layout.pargb.mode, VertexLayoutMode::Enable);
        assert_eq!(layout.color_type, VertexColorType::U8);
    }

    #[test]
    fn test_vertex_layout_alpha() {
        let mut gr = create_test_gr();
        gr.vertex_layout(GR_PARAM_A, 28, GR_PARAM_ENABLE);

        let layout = gr.get_vertex_layout();
        assert_eq!(layout.a.offset, 28);
        assert_eq!(layout.a.mode, VertexLayoutMode::Enable);
        assert_eq!(layout.color_type, VertexColorType::Float);
    }

    #[test]
    fn test_vertex_layout_texture_coords() {
        let mut gr = create_test_gr();
        gr.vertex_layout(GR_PARAM_ST0, 32, GR_PARAM_ENABLE);
        gr.vertex_layout(GR_PARAM_ST1, 40, GR_PARAM_ENABLE);

        let layout = gr.get_vertex_layout();
        assert_eq!(layout.st0.offset, 32);
        assert_eq!(layout.st0.mode, VertexLayoutMode::Enable);
        assert_eq!(layout.st1.offset, 40);
        assert_eq!(layout.st1.mode, VertexLayoutMode::Enable);
    }

    #[test]
    fn test_vertex_layout_q_coords() {
        let mut gr = create_test_gr();
        gr.vertex_layout(GR_PARAM_Q, 48, GR_PARAM_ENABLE);
        gr.vertex_layout(GR_PARAM_Q0, 52, GR_PARAM_ENABLE);
        gr.vertex_layout(GR_PARAM_Q1, 56, GR_PARAM_ENABLE);

        let layout = gr.get_vertex_layout();
        assert_eq!(layout.q.offset, 48);
        assert_eq!(layout.q.mode, VertexLayoutMode::Enable);
        assert_eq!(layout.q0.offset, 52);
        assert_eq!(layout.q0.mode, VertexLayoutMode::Enable);
        assert_eq!(layout.q1.offset, 56);
        assert_eq!(layout.q1.mode, VertexLayoutMode::Enable);
    }

    #[test]
    fn test_vertex_layout_disable() {
        let mut gr = create_test_gr();
        gr.vertex_layout(GR_PARAM_RGB, 16, GR_PARAM_ENABLE);
        gr.vertex_layout(GR_PARAM_RGB, 16, GR_PARAM_DISABLE);

        let layout = gr.get_vertex_layout();
        assert_eq!(layout.rgb.mode, VertexLayoutMode::Disable);
    }

    #[test]
    fn test_vertex_layout_reset() {
        let mut gr = create_test_gr();
        // Set up some layout
        gr.vertex_layout(GR_PARAM_XY, 0, GR_PARAM_ENABLE);
        gr.vertex_layout(GR_PARAM_RGB, 16, GR_PARAM_ENABLE);
        gr.vertex_layout(GR_PARAM_ST0, 32, GR_PARAM_ENABLE);

        // Reset
        gr.reset_vertex_layout();

        // All should be disabled
        let layout = gr.get_vertex_layout();
        assert_eq!(layout.xy.mode, VertexLayoutMode::Disable);
        assert_eq!(layout.rgb.mode, VertexLayoutMode::Disable);
        assert_eq!(layout.st0.mode, VertexLayoutMode::Disable);
        assert_eq!(layout.color_type, VertexColorType::Float);
    }

    #[test]
    fn test_vertex_layout_typical_setup() {
        let mut gr = create_test_gr();

        // Typical Glide vertex setup like in test programs
        gr.vertex_layout(GR_PARAM_XY, 0, GR_PARAM_ENABLE);
        gr.vertex_layout(GR_PARAM_RGB, 16, GR_PARAM_ENABLE);
        gr.vertex_layout(GR_PARAM_A, 28, GR_PARAM_ENABLE);
        gr.vertex_layout(GR_PARAM_ST0, 36, GR_PARAM_ENABLE);

        let layout = gr.get_vertex_layout();
        assert_eq!(layout.xy.offset, 0);
        assert_eq!(layout.xy.mode, VertexLayoutMode::Enable);
        assert_eq!(layout.rgb.offset, 16);
        assert_eq!(layout.rgb.mode, VertexLayoutMode::Enable);
        assert_eq!(layout.a.offset, 28);
        assert_eq!(layout.a.mode, VertexLayoutMode::Enable);
        assert_eq!(layout.st0.offset, 36);
        assert_eq!(layout.st0.mode, VertexLayoutMode::Enable);
        assert_eq!(layout.color_type, VertexColorType::Float);
    }

    // ==================== grReset Tests ====================

    // GR_STATS_* and GR_VERTEX_PARAMETER constants
    const GR_STATS_LINES: u32 = 0x17;
    const GR_STATS_PIXELS: u32 = 0x1d;
    const GR_STATS_POINTS: u32 = 0x1e;
    const GR_STATS_TRIANGLES: u32 = 0x21;
    const GR_VERTEX_PARAMETER: u32 = 0x29;

    #[test]
    fn test_reset_stats_points() {
        let mut gr = create_test_gr();
        let result = gr.reset(GR_STATS_POINTS);
        assert_eq!(result, FxBool::True);
    }

    #[test]
    fn test_reset_stats_lines() {
        let mut gr = create_test_gr();
        let result = gr.reset(GR_STATS_LINES);
        assert_eq!(result, FxBool::True);
    }

    #[test]
    fn test_reset_stats_pixels() {
        let mut gr = create_test_gr();
        let result = gr.reset(GR_STATS_PIXELS);
        assert_eq!(result, FxBool::True);
    }

    #[test]
    fn test_reset_stats_triangles() {
        let mut gr = create_test_gr();
        let result = gr.reset(GR_STATS_TRIANGLES);
        assert_eq!(result, FxBool::True);
    }

    #[test]
    fn test_reset_vertex_parameter() {
        let mut gr = create_test_gr();

        // Set up some vertex layout
        gr.vertex_layout(GR_PARAM_XY, 0, GR_PARAM_ENABLE);
        gr.vertex_layout(GR_PARAM_RGB, 16, GR_PARAM_ENABLE);
        gr.vertex_layout(GR_PARAM_ST0, 32, GR_PARAM_ENABLE);

        // Verify layout is set
        let layout = gr.get_vertex_layout();
        assert_eq!(layout.xy.mode, VertexLayoutMode::Enable);

        // Reset vertex parameters
        let result = gr.reset(GR_VERTEX_PARAMETER);
        assert_eq!(result, FxBool::True);

        // All should be disabled now
        let layout = gr.get_vertex_layout();
        assert_eq!(layout.xy.mode, VertexLayoutMode::Disable);
        assert_eq!(layout.rgb.mode, VertexLayoutMode::Disable);
        assert_eq!(layout.st0.mode, VertexLayoutMode::Disable);
        assert_eq!(layout.color_type, VertexColorType::Float);
    }

    #[test]
    fn test_reset_unknown_parameter() {
        let mut gr = create_test_gr();
        let result = gr.reset(0xFF); // Invalid parameter
        assert_eq!(result, FxBool::False);
    }
}
