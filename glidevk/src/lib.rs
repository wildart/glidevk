#![allow(non_snake_case, dead_code)]

pub mod api;
pub mod config;
pub mod glidesys;
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
    x1: f32, y1: f32, x2: f32, y2: f32,
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
    x: f32, y: f32, w: f32, h: f32, digit: u8,
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
        push_quad(vertices, x, y + mid - t / 2.0, x + w, y + mid + t / 2.0, color);
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
    let qualifier = if fatal == FxBool::True {
        "Fatal"
    } else {
        "Recoverable"
    };
    // Safety: we are always the producer of this string, so it’s always UTF-8 encoded.
    let string = unsafe { CStr::from_ptr(string) }.to_str().unwrap();
    eprintln!("{qualifier} error: {string}");
}

#[derive(Debug)]
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

    // Clear color (RGBA floats)
    clear_r: f32,
    clear_g: f32,
    clear_b: f32,
    clear_a: f32,
    clear_depth: f32,

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
    tex_combine_function: [TextureCombineFnc; GLIDE_NUM_TMU],
    tex_combine_factor: [CombineFactor; GLIDE_NUM_TMU],

    // Texture table state
    tex_table_type: TextureTableType,

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
            clear_r: 0.0,
            clear_g: 0.0,
            clear_b: 0.0,
            clear_a: 1.0,
            clear_depth: 1.0,
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
            tex_source: [0; GLIDE_NUM_TMU],
            tex_filter_mode_min: [TextureFilterMode::PointSampled; GLIDE_NUM_TMU],
            tex_filter_mode_mag: [TextureFilterMode::PointSampled; GLIDE_NUM_TMU],
            tex_clamp_mode_s: [TextureClampMode::Wrap; GLIDE_NUM_TMU],
            tex_clamp_mode_t: [TextureClampMode::Wrap; GLIDE_NUM_TMU],
            tex_mipmap_mode: [MipMapMode::Disable; GLIDE_NUM_TMU],
            tex_lod_bias: [0.0; GLIDE_NUM_TMU],
            tex_combine_function: [TextureCombineFnc::Zero; GLIDE_NUM_TMU],
            tex_combine_factor: [CombineFactor::Zero; GLIDE_NUM_TMU],
            // Default texture table type
            tex_table_type: TextureTableType::Palette,
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
    /// Whether LFB write was performed this frame (skip render in buffer_swap)
    lfb_written: bool,
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
            lfb_written: false,
        }
    }
}

pub struct Gr {
    /// Wrapper configuration loaded from glidevk.toml
    config: config::Config,
    error_callback: unsafe extern "C" fn(*const c_char, FxBool),
    vulkan_instance: VulkanInstance,
    vulkan_app: Option<VulkanApp>,
    window: Option<Window>,
    surface: vk::SurfaceKHR,
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
            "GlideVK config: windowed={}, keep_aspect_ratio={}, texture_memory={}MB",
            config.window.windowed,
            config.window.keep_aspect_ratio,
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
        unsafe {
            assert_eq!(refresh, ScreenRefresh::R60Hz);

            // Get game's requested resolution
            let (game_width, game_height) = res.into();
            self.state.screen_width = game_width as u32;
            self.state.screen_height = game_height as u32;
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

            let _flip = match origin {
                OriginLocation::UpperLeft => false,
                OriginLocation::LowerLeft => true,
                OriginLocation::Any => panic!("OriginLocation::Any is currently unsupported."),
            };

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
            let fence_info =
                vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
            let mut image_available_semaphores = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
            let mut in_flight_fences = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
            for _ in 0..MAX_FRAMES_IN_FLIGHT {
                image_available_semaphores.push(unsafe {
                    logical_device
                        .create_semaphore(&semaphore_info, None)
                        .expect("Failed to create image available semaphore")
                });
                in_flight_fences.push(unsafe {
                    logical_device
                        .create_fence(&fence_info, None)
                        .expect("Failed to create in-flight fence")
                });
            }
            let swapchain_image_count = swapchain_images.len();
            let mut render_finished_semaphores =
                Vec::with_capacity(swapchain_image_count);
            for _ in 0..swapchain_image_count {
                render_finished_semaphores.push(unsafe {
                    logical_device
                        .create_semaphore(&semaphore_info, None)
                        .expect("Failed to create render finished semaphore")
                });
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
                    vk::MemoryPropertyFlags::HOST_VISIBLE
                        | vk::MemoryPropertyFlags::HOST_COHERENT,
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
            log::debug!("Vulkan app initialized successfully");
        }

        log::debug!("sst_win_open completed");
        return FxBool::True as u32; // context
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
        // If LFB write was performed, the image was already presented in lfb_unlock.
        // Just clear the flag and skip the normal render path.
        if self.lfb_lock_state.lfb_written {
            self.lfb_lock_state.lfb_written = false;
            self.vertex_batch.clear();
            self.frame_textures.clear();
            return Some(());
        }

        let frame_start = std::time::Instant::now();

        // Finalize vertex batch and get draw calls (copy to avoid borrow issues)
        let draw_calls: Vec<utility::vertex::DrawCall> = self.vertex_batch.finalize().to_vec();

        // Collect textures needed this frame (accumulated during draw calls)
        let needed_textures: std::collections::HashSet<u32> = self.frame_textures.clone();
        if !needed_textures.is_empty() {
            log::trace!("Frame needs {} unique textures: {:?}",
                needed_textures.len(),
                needed_textures.iter().take(5).map(|a| format!("{:#x}", a)).collect::<Vec<_>>()
            );
        }

        // Batch-upload all missing textures in a single GPU submission
        {
            let mut to_upload: Vec<(u32, utility::texture::TextureData, Option<utility::texture::PaletteTable>)> = Vec::new();

            for &addr in &needed_textures {
                if addr == 0 {
                    continue;
                }
                let needs_upload = self.vulkan_app.as_ref()
                    .map(|app| !app.texture_cache.has_texture(addr))
                    .unwrap_or(false);

                if needs_upload {
                    // Find which TMU has this texture and get its data
                    for tmu_idx in 0..GLIDE_NUM_TMU {
                        if let Some(tex) = self.texture_memory[tmu_idx].get_texture(addr) {
                            let palette = self.texture_memory[tmu_idx]
                                .get_palette(0)
                                .map(|p| utility::texture::PaletteTable { entries: p.entries.clone() });
                            to_upload.push((
                                addr,
                                utility::texture::TextureData::new(
                                    tex.width, tex.height, tex.format, tex.data.clone(),
                                ),
                                palette,
                            ));
                            break;
                        }
                    }
                }
            }

            if !to_upload.is_empty() {
                if let Some(app) = &mut self.vulkan_app {
                    app.texture_cache.batch_upload(
                        &app.device,
                        &self.vulkan_instance.instance,
                        app.physical_device,
                        app.command_pool,
                        app.graphics_queue,
                        to_upload,
                    );
                }
            }
        }

        // Resolve texture addresses → descriptor slot indices now that all textures are uploaded
        if let Some(app) = &self.vulkan_app {
            let mut unresolved = 0u32;
            for v in &mut self.vertex_batch.vertices {
                let addr = v.texture_index;
                let slot = app.texture_cache.get_texture_slot(addr);
                if slot == 0 && addr != 0 {
                    unresolved += 1;
                }
                v.texture_index = slot;
            }
            if unresolved > 0 {
                log::warn!(
                    "buffer_swap: {} vertices with unresolved textures (slot 0 / placeholder)",
                    unresolved
                );
            }
            // Log texture stats for first 60 frames
            static FRAME_NUM: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let frame = FRAME_NUM.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if frame < 60 {
                let unique_slots: std::collections::HashSet<u32> = self.vertex_batch.vertices.iter()
                    .map(|v| v.texture_index).collect();
                log::warn!(
                    "Frame {}: {} unique textures in {} vertices, {} draw calls, cache has {} textures",
                    frame, unique_slots.len(), self.vertex_batch.vertex_count(),
                    draw_calls.len(), app.texture_cache.texture_count()
                );
            }
        }

        log::trace!(
            "buffer_swap: {} draw calls, {} vertices",
            draw_calls.len(),
            self.vertex_batch.vertex_count()
        );

        let vertex_count = self.vertex_batch.vertex_count();
        let vertex_data = self.vertex_batch.as_bytes().to_vec();
        let clear_r = self.state.clear_r;
        let clear_g = self.state.clear_g;
        let clear_b = self.state.clear_b;
        let clear_a = self.state.clear_a;
        let clear_depth = self.state.clear_depth;
        let screen_width = self.state.screen_width;
        let screen_height = self.state.screen_height;

        // Capture chromakey color (enabled state is now per-draw-call)
        let (ck_r, ck_g, ck_b, _) = self
            .state
            .chromakey_value
            .to_rgba_f(self.state.color_format);

        // Capture fog state
        let fog_mode = self.state.fog_mode as i32;
        let (fog_r, fog_g, fog_b, _) = self
            .state
            .fog_color
            .to_rgba_f(self.state.color_format);

        if let Some(app) = &mut self.vulkan_app {
            unsafe {
                // Wait for this frame-in-flight slot to be available
                let frame = app.current_frame;
                app.device
                    .wait_for_fences(&[app.in_flight_fences[frame]], true, u64::MAX)
                    .expect("Failed to wait for fence");
                app.device
                    .reset_fences(&[app.in_flight_fences[frame]])
                    .expect("Failed to reset fence");

                // Flush deferred texture/descriptor destruction now that this
                // frame slot's GPU work is complete
                app.texture_cache.flush_pending_destroy(&app.device);

                // Evict unused textures — must happen after fence wait so no
                // in-flight frame in THIS slot is still referencing them.
                // Wait for ALL slots to ensure the other slot is also done.
                if app.texture_cache.needs_eviction() {
                    for fence in &app.in_flight_fences {
                        let _ = app.device.wait_for_fences(&[*fence], true, u64::MAX);
                    }
                    app.texture_cache
                        .evict_if_needed(&app.device, &needed_textures);
                }

                // Acquire next swapchain image with retry loop for NOT_READY
                let image_index: u32 = loop {
                    match app.swapchain_loader.acquire_next_image(
                        app.swapchain,
                        u64::MAX,
                        app.image_available_semaphores[frame],
                        vk::Fence::null(),
                    ) {
                        Ok((idx, _)) => {
                            app.current_image_index = idx;
                            break idx;
                        }
                        Err(vk::Result::NOT_READY) => {
                            // Image not ready yet, wait a bit and retry
                            std::thread::sleep(std::time::Duration::from_millis(1));
                            continue;
                        }
                        Err(e) => panic!("Failed to acquire next image: {:?}", e),
                    }
                };

                // Upload vertex data to this frame's vertex buffer
                if vertex_count > 0 {
                    utility::vertex::upload_to_buffer(
                        &app.device,
                        &app.vertex_buffers[frame],
                        &vertex_data,
                    );
                }

                // Record command buffer for this frame
                let cmd = app.command_buffers[image_index as usize];

                // Reset and begin command buffer
                app.device
                    .reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                    .expect("Failed to reset command buffer");

                let begin_info = vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
                app.device
                    .begin_command_buffer(cmd, &begin_info)
                    .expect("Failed to begin command buffer");

                // Begin render pass with clear values
                let clear_values = [
                    vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [clear_r, clear_g, clear_b, clear_a],
                        },
                    },
                    vk::ClearValue {
                        depth_stencil: vk::ClearDepthStencilValue {
                            depth: clear_depth,
                            stencil: 0,
                        },
                    },
                ];

                let render_pass_info = vk::RenderPassBeginInfo::default()
                    .render_pass(app.render_pass)
                    .framebuffer(app.swapchain_framebuffers[image_index as usize])
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: app.swapchain_extent,
                    })
                    .clear_values(&clear_values);

                app.device.cmd_begin_render_pass(
                    cmd,
                    &render_pass_info,
                    vk::SubpassContents::INLINE,
                );

                // Bind initial pipeline (will switch per draw call if blend mode changes)
                let default_pipeline = app.graphics_pipelines[&utility::vertex::BlendMode::Opaque];
                app.device.cmd_bind_pipeline(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    default_pipeline,
                );
                let mut current_blend_mode = utility::vertex::BlendMode::Opaque;

                // Set viewport and scissor
                let viewport = vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: app.swapchain_extent.width as f32,
                    height: app.swapchain_extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                };
                app.device.cmd_set_viewport(cmd, 0, &[viewport]);

                let scissor = vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: app.swapchain_extent,
                };
                app.device.cmd_set_scissor(cmd, 0, &[scissor]);

                // Draw triangles if we have any
                if vertex_count > 0 && !draw_calls.is_empty() {
                    {
                        // Bind this frame's vertex buffer
                        app.device.cmd_bind_vertex_buffers(
                            cmd,
                            0,
                            &[app.vertex_buffers[frame].buffer],
                            &[0],
                        );

                        // Bind the single shared descriptor set ONCE (texture selection is per-vertex)
                        let descriptor_set = app.texture_cache.get_descriptor_set();
                        app.device.cmd_bind_descriptor_sets(
                            cmd,
                            vk::PipelineBindPoint::GRAPHICS,
                            app.pipeline_layout,
                            0,
                            &[descriptor_set],
                            &[],
                        );

                        // Track previous state to skip redundant GPU commands
                        let mut prev_push_bytes: [u8; 96] = [0; 96];
                        let mut first_draw = true;

                        // Issue draw calls (only break on blend_mode/chromakey changes)
                        for draw_call in &draw_calls {
                            // Switch pipeline if blend mode changed
                            if draw_call.blend_mode != current_blend_mode {
                                if let Some(&pipeline) = app.graphics_pipelines.get(&draw_call.blend_mode) {
                                    app.device.cmd_bind_pipeline(
                                        cmd,
                                        vk::PipelineBindPoint::GRAPHICS,
                                        pipeline,
                                    );
                                    current_blend_mode = draw_call.blend_mode;
                                }
                            }

                            // Build push constants (no texture_size -- computed in shader)
                            let push_constants = utility::vertex::PushConstants {
                                screen_size: [screen_width as f32, screen_height as f32],
                                chromakey_r: ck_r,
                                chromakey_g: ck_g,
                                chromakey_b: ck_b,
                                fog_mode,
                                fog_color_r: fog_r,
                                fog_color_g: fog_g,
                                fog_color_b: fog_b,
                            };

                            // Only push constants if they changed
                            let push_data = std::slice::from_raw_parts(
                                &push_constants as *const _ as *const u8,
                                std::mem::size_of::<utility::vertex::PushConstants>(),
                            );
                            if first_draw || push_data != &prev_push_bytes[..push_data.len()] {
                                app.device.cmd_push_constants(
                                    cmd,
                                    app.pipeline_layout,
                                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                                    0,
                                    push_data,
                                );
                                prev_push_bytes[..push_data.len()].copy_from_slice(push_data);
                            }

                            first_draw = false;

                            // Draw this batch
                            app.device.cmd_draw(
                                cmd,
                                draw_call.vertex_count,
                                1,
                                draw_call.start_vertex,
                                0,
                            );
                            log::trace!(
                                "buffer_swap: drew {} vertices",
                                draw_call.vertex_count,
                            );
                        }
                        log::trace!("buffer_swap: completed {} draw calls", draw_calls.len());
                    }
                }

                // Draw FPS counter overlay if enabled
                if self.config.render.fps != config::FpsPosition::Off {
                    self.fps_counter.frame_count += 1;
                    let now = std::time::Instant::now();
                    let elapsed = now.duration_since(self.fps_counter.last_time);
                    if elapsed.as_secs_f32() >= 1.0 {
                        self.fps_counter.display_fps = (self.fps_counter.frame_count as f32
                            / elapsed.as_secs_f32())
                            as u32;
                        self.fps_counter.frame_count = 0;
                        self.fps_counter.last_time = now;
                    }

                    let fps = self.fps_counter.display_fps;
                    if fps > 0 {
                        let sw = screen_width as f32;
                        let sh = screen_height as f32;
                        let digits = fps_digits(fps);
                        let digit_w = 6.0;
                        let digit_h = 10.0;
                        let padding = 2.0;
                        let total_w =
                            digits.len() as f32 * (digit_w + padding) - padding + padding * 2.0;

                        let (start_x, start_y) = match self.config.render.fps {
                            config::FpsPosition::TopLeft => (padding, padding),
                            config::FpsPosition::TopRight => (sw - total_w - padding, padding),
                            config::FpsPosition::BottomLeft => (padding, sh - digit_h - padding * 3.0),
                            config::FpsPosition::BottomRight => {
                                (sw - total_w - padding, sh - digit_h - padding * 3.0)
                            }
                            config::FpsPosition::Off => unreachable!(),
                        };

                        // Build FPS overlay vertices
                        let mut fps_vertices = Vec::new();
                        // Background quad
                        let bg_x1 = start_x - padding;
                        let bg_y1 = start_y - padding;
                        let bg_x2 = start_x + total_w;
                        let bg_y2 = start_y + digit_h + padding;
                        push_quad(
                            &mut fps_vertices,
                            bg_x1, bg_y1, bg_x2, bg_y2,
                            [0.0, 0.0, 0.0, 0.6],
                        );
                        // Digit quads (7-segment)
                        let mut cx = start_x;
                        for &d in &digits {
                            draw_7seg_digit(
                                &mut fps_vertices,
                                cx, start_y, digit_w, digit_h, d,
                                [0.0, 1.0, 0.0, 1.0],
                            );
                            cx += digit_w + padding;
                        }

                        // Upload FPS vertices to current frame's buffer (appended after game vertices)
                        let fps_start = vertex_count as u32;
                        let fps_count = fps_vertices.len() as u32;
                        let fps_bytes: &[u8] = unsafe {
                            std::slice::from_raw_parts(
                                fps_vertices.as_ptr() as *const u8,
                                fps_vertices.len()
                                    * std::mem::size_of::<utility::vertex::GpuVertex>(),
                            )
                        };
                        let offset = vertex_count
                            * std::mem::size_of::<utility::vertex::GpuVertex>();
                        let vb = &app.vertex_buffers[frame];
                        if offset + fps_bytes.len() <= vb.size as usize {
                            let data_ptr = app.device.map_memory(
                                vb.memory,
                                offset as vk::DeviceSize,
                                fps_bytes.len() as vk::DeviceSize,
                                vk::MemoryMapFlags::empty(),
                            ).expect("Failed to map memory for FPS overlay");
                            std::ptr::copy_nonoverlapping(
                                fps_bytes.as_ptr(),
                                data_ptr as *mut u8,
                                fps_bytes.len(),
                            );
                            app.device.unmap_memory(vb.memory);

                            // Draw with CF_LOCAL (vertex color only), alpha blend, placeholder texture
                            let fps_push = utility::vertex::PushConstants {
                                screen_size: [sw, sh],
                                chromakey_r: 0.0,
                                chromakey_g: 0.0,
                                chromakey_b: 0.0,
                                fog_mode: 0,
                                fog_color_r: 0.0,
                                fog_color_g: 0.0,
                                fog_color_b: 0.0,
                            };
                            let fps_push_data = std::slice::from_raw_parts(
                                &fps_push as *const _ as *const u8,
                                std::mem::size_of::<utility::vertex::PushConstants>(),
                            );
                            app.device.cmd_push_constants(
                                cmd,
                                app.pipeline_layout,
                                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                                0,
                                fps_push_data,
                            );

                            // Switch to alpha blend pipeline for transparent background
                            let blend_pipeline =
                                app.graphics_pipelines[&utility::vertex::BlendMode::AlphaBlend];
                            app.device.cmd_bind_pipeline(
                                cmd,
                                vk::PipelineBindPoint::GRAPHICS,
                                blend_pipeline,
                            );

                            // Descriptor set already bound (shared set, texture_index=0 is placeholder)

                            app.device.cmd_draw(cmd, fps_count, 1, fps_start, 0);
                        }
                    }
                }

                // End render pass and command buffer
                app.device.cmd_end_render_pass(cmd);
                app.device
                    .end_command_buffer(cmd)
                    .expect("Failed to end command buffer");

                // Submit command buffer
                let wait_semaphores = [app.image_available_semaphores[frame]];
                let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
                let signal_semaphores = [app.render_finished_semaphores[image_index as usize]];
                let command_buffers = [cmd];

                let submit_info = vk::SubmitInfo::default()
                    .wait_semaphores(&wait_semaphores)
                    .wait_dst_stage_mask(&wait_stages)
                    .command_buffers(&command_buffers)
                    .signal_semaphores(&signal_semaphores);

                app.device
                    .queue_submit(app.graphics_queue, &[submit_info], app.in_flight_fences[frame])
                    .expect("Failed to submit draw command buffer");

                // Present the image
                let swapchains = [app.swapchain];
                let image_indices = [image_index];
                let present_info = vk::PresentInfoKHR::default()
                    .wait_semaphores(&signal_semaphores)
                    .swapchains(&swapchains)
                    .image_indices(&image_indices);

                app.swapchain_loader
                    .queue_present(app.present_queue, &present_info)
                    .expect("Failed to present");

                app.last_presented_image_index = image_index;

                // Advance to next frame-in-flight slot
                app.current_frame = (app.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;

                let frame_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
                if frame_ms > 18.0 {
                    log::warn!(
                        "Slow frame: {:.1}ms, {} draw calls, {} vertices",
                        frame_ms, draw_calls.len(), vertex_count
                    );
                }
            }
            Some(())
        } else {
            log::error!("Vulkan application is not initialized. Cannot swap buffers.");
            None
        }
    }

    /// Buffer swap for LFB write case - image already acquired and written, just present
    fn buffer_swap_lfb(&mut self) -> Option<()> {
        if let Some(app) = &mut self.vulkan_app {
            unsafe {
                let image_index = app.current_image_index;
                log::trace!(
                    "buffer_swap_lfb: presenting LFB-written image {}",
                    image_index
                );

                // Wait for the image_available semaphore (from acquire in lfb_upload)
                // and present directly
                let frame = app.current_frame;
                let wait_semaphores = [app.image_available_semaphores[frame]];
                let swapchains = [app.swapchain];
                let image_indices = [image_index];
                let present_info = vk::PresentInfoKHR::default()
                    .wait_semaphores(&wait_semaphores)
                    .swapchains(&swapchains)
                    .image_indices(&image_indices);

                app.swapchain_loader
                    .queue_present(app.present_queue, &present_info)
                    .expect("Failed to present");

                app.last_presented_image_index = image_index;

                // Wait for present to complete so next frame's fence wait doesn't hang
                app.device
                    .queue_wait_idle(app.present_queue)
                    .expect("Failed to wait for present queue");

                // Advance to next frame-in-flight slot
                app.current_frame = (app.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
            }
            Some(())
        } else {
            log::error!("Vulkan application is not initialized. Cannot swap buffers.");
            None
        }
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
        // Convert color to RGBA float values based on the current color format
        let (r, g, b, _a) = color.to_rgba_f(self.state.color_format);
        let a = 1.0; // Alpha is ignored as per the description

        // Normalize depth value to Vulkan's [0.0, 1.0] range
        // In Glide, 0xFFFF = "farthest" to allow all fragments to pass initially
        // Our shader inverts oow so: depth = 1.0 - oow, meaning closer = smaller depth
        // Clearing to 1.0 (farthest in Vulkan) allows LESS_OR_EQUAL to pass all fragments initially
        let normalized_depth = (depth as f32) / 65535.0;

        // Store clear values in state for use in buffer_swap
        self.state.clear_r = r;
        self.state.clear_g = g;
        self.state.clear_b = b;
        self.state.clear_a = a;
        self.state.clear_depth = normalized_depth;

        // Clear the vertex batch and frame texture set for the new frame
        self.vertex_batch.clear();
        self.frame_textures.clear();

        log::trace!(
            "Buffer clear values set: color=[{}, {}, {}, {}], depth={}",
            r,
            g,
            b,
            a,
            normalized_depth
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
        self.state.depth_mask = mask == FxBool::True;
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
        self.state.color_mask_rgb = rgb == FxBool::True;
        self.state.color_mask_alpha = alpha == FxBool::True;
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

        // Map Glide blend args to a pipeline blend mode
        let blend_mode = match (rgb_sf, rgb_df) {
            (AlphaBlendFnc::One, AlphaBlendFnc::Zero) => utility::vertex::BlendMode::Opaque,
            (AlphaBlendFnc::One, AlphaBlendFnc::One) => utility::vertex::BlendMode::Additive,
            (AlphaBlendFnc::Zero, AlphaBlendFnc::SrcColor) => utility::vertex::BlendMode::Multiplicative,
            (AlphaBlendFnc::SrcAlpha, AlphaBlendFnc::OneMinusSrcAlpha) => utility::vertex::BlendMode::AlphaBlend,
            _ => utility::vertex::BlendMode::AlphaBlend, // fallback
        };
        self.vertex_batch.set_blend_mode(blend_mode);

        log::debug!(
            "Alpha blend function set to: RGB({:?}, {:?}), Alpha({:?}, {:?}) → {:?}",
            rgb_sf,
            rgb_df,
            alpha_sf,
            alpha_df,
            blend_mode
        );
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
        self.state.color_combine_invert = invert == FxBool::True;
        log::trace!(
            "Color combine: function={:?}, factor={:?}, local={:?}, other={:?}, invert={:?}",
            function, factor, local, other, invert
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
        self.state.alpha_combine_invert = invert == FxBool::True;
        log::trace!(
            "Alpha combine: function={:?}, factor={:?}, local={:?}, other={:?}, invert={:?}",
            function, factor, local, other, invert
        );
    }

    /// Pack current combine state into a u32 for per-vertex storage
    fn current_combine_flags(&self) -> u32 {
        let ccf = self.state.color_combine_function as u32 & 0x1F;
        let acf = (self.state.alpha_combine_function as u32 & 0x1F) << 5;
        let ccfr = (self.state.color_combine_factor as u32 & 0xF) << 10;
        let acfr = (self.state.alpha_combine_factor as u32 & 0xF) << 14;
        let cco = (self.state.color_combine_other as u32 & 0x3) << 18;
        let acl = (self.state.alpha_combine_local as u32 & 0x3) << 20;
        let ccl = (self.state.color_combine_local as u32 & 0x3) << 22;
        let aco = (self.state.alpha_combine_other as u32 & 0x3) << 24;
        let ck = if matches!(self.state.chromakey_mode, ChromakeyMode::Enable) { 1u32 << 26 } else { 0 };
        ccf | acf | ccfr | acfr | cco | acl | ccl | aco | ck
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

    /// Ensure the active texture for a TMU is uploaded to Vulkan
    /// Returns the texture address if texturing is enabled, or 0 if not
    fn ensure_texture_uploaded(&mut self, tmu_idx: usize) -> u32 {
        let tex_addr = self.state.tex_source[tmu_idx];
        if tex_addr == 0 {
            return 0;
        }

        // Check if texture is already in Vulkan cache
        let needs_upload = self
            .vulkan_app
            .as_ref()
            .map(|app| !app.texture_cache.has_texture(tex_addr))
            .unwrap_or(false);

        if needs_upload {
            // Get texture data from CPU memory (clone to avoid borrow issues)
            let texture_info = self.texture_memory[tmu_idx]
                .get_texture(tex_addr)
                .map(|t| (t.width, t.height, t.format, t.data.clone()));

            let palette_entries = self.texture_memory[tmu_idx]
                .get_palette(0)
                .map(|p| p.entries.clone());

            if texture_info.is_none() {
                log::warn!(
                    "Texture {:#x} not found in CPU memory for TMU{} (using placeholder)",
                    tex_addr,
                    tmu_idx
                );
            }

            if let (Some((width, height, format, data)), Some(app)) =
                (texture_info, &mut self.vulkan_app)
            {
                log::trace!(
                    "Uploading texture {:#x} to Vulkan ({}x{}, {:?})",
                    tex_addr,
                    width,
                    height,
                    format
                );

                // Create temporary TextureData and PaletteTable for upload
                let texture_data = utility::texture::TextureData::new(width, height, format, data);
                let palette =
                    palette_entries.map(|entries| utility::texture::PaletteTable { entries });

                app.texture_cache.upload_texture(
                    &app.device,
                    &self.vulkan_instance.instance,
                    app.physical_device,
                    app.command_pool,
                    app.graphics_queue,
                    tex_addr,
                    &texture_data,
                    palette.as_ref(),
                );
            }
        }
        tex_addr
    }

    /// Ensure a texture is uploaded to Vulkan by searching all TMUs for it
    fn ensure_texture_uploaded_by_addr(&mut self, tex_addr: u32) {
        if tex_addr == 0 {
            return;
        }

        // Check if texture is already in Vulkan cache
        let needs_upload = self
            .vulkan_app
            .as_ref()
            .map(|app| !app.texture_cache.has_texture(tex_addr))
            .unwrap_or(false);

        if !needs_upload {
            return;
        }

        // Search all TMUs for this texture address
        let mut texture_info = None;
        let mut palette_entries = None;

        for tmu_idx in 0..GLIDE_NUM_TMU {
            if let Some(tex) = self.texture_memory[tmu_idx].get_texture(tex_addr) {
                texture_info = Some((tex.width, tex.height, tex.format, tex.data.clone()));
                palette_entries = self.texture_memory[tmu_idx]
                    .get_palette(0)
                    .map(|p| p.entries.clone());
                break;
            }
        }

        if texture_info.is_none() {
            log::warn!(
                "Texture {:#x} not found in CPU memory for any TMU - will use placeholder",
                tex_addr
            );
            return;
        }

        // Validate texture dimensions before upload
        if let Some((width, height, _, _)) = &texture_info {
            if *width == 0 || *height == 0 {
                log::trace!(
                    "Texture {:#x} has invalid dimensions ({}x{}), skipping",
                    tex_addr,
                    width,
                    height
                );
                return;
            }
        }

        if let (Some((width, height, format, data)), Some(app)) =
            (texture_info, &mut self.vulkan_app)
        {
            log::trace!(
                "Uploading texture {:#x} to Vulkan ({}x{}, {:?})",
                tex_addr,
                width,
                height,
                format
            );

            let texture_data = utility::texture::TextureData::new(width, height, format, data);
            let palette = palette_entries.map(|entries| utility::texture::PaletteTable { entries });

            app.texture_cache.upload_texture(
                &app.device,
                &self.vulkan_instance.instance,
                app.physical_device,
                app.command_pool,
                app.graphics_queue,
                tex_addr,
                &texture_data,
                palette.as_ref(),
            );
        }
    }

    /// grTexSource - specify the current texture source for rendering
    pub fn tex_source(
        &mut self,
        tmu: ChipID,
        start_address: FxU32,
        even_odd: FxU32,
        info: &TexInfo,
    ) {
        let tmu_idx = tmu as usize;
        if tmu_idx < GLIDE_NUM_TMU {
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
        let tmu_idx = tmu as usize;
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
        let tmu_idx = tmu as usize;
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
        let tmu_idx = tmu as usize;
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
        let tmu_idx = tmu as usize;
        if tmu_idx < GLIDE_NUM_TMU {
            self.state.tex_lod_bias[tmu_idx] = bias;
            log::debug!("Texture LOD bias set for TMU{}: {}", tmu_idx, bias);
        }
    }

    /// grTexCombine - set texture combine mode
    pub fn tex_combine(
        &mut self,
        tmu: ChipID,
        rgb_function: TextureCombineFnc,
        rgb_factor: CombineFactor,
        alpha_function: TextureCombineFnc,
        alpha_factor: CombineFactor,
        rgb_invert: FxBool,
        alpha_invert: FxBool,
    ) {
        let tmu_idx = tmu as usize;
        if tmu_idx < GLIDE_NUM_TMU {
            self.state.tex_combine_function[tmu_idx] = rgb_function;
            self.state.tex_combine_factor[tmu_idx] = rgb_factor;
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
        let tmu_idx = tmu as usize;
        if tmu_idx < GLIDE_NUM_TMU {
            (self.texture_memory[tmu_idx].memory_available() as FxU32).saturating_sub(8)
        } else {
            0
        }
    }

    /// grTexTextureMemRequired - return bytes required for texture
    pub fn tex_texture_mem_required(&self, even_odd: FxU32, info: &TexInfo) -> FxU32 {
        let _ = even_odd;
        info.get_size() as FxU32
    }

    /// Get texture memory size for a TMU (in bytes)
    /// GlideVK extension - not part of original Glide API
    pub fn get_texture_memory_size(&self, tmu: ChipID) -> usize {
        let tmu_idx = tmu as usize;
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
        let tmu_idx = tmu as usize;
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

    /// grTexDownloadMipMap - download texture mipmap to TMU memory
    pub fn tex_download_mipmap(
        &mut self,
        tmu: ChipID,
        start_address: FxU32,
        _even_odd: FxU32,
        info: &TexInfo,
    ) {
        let tmu_idx = tmu as usize;
        if tmu_idx >= GLIDE_NUM_TMU {
            return;
        }

        let (width, height) = info.get_dimensions();
        let _size = info.get_size();
        let format = info.format();

        log::trace!(
            "tex_download_mipmap: addr={:#x}, {}x{}, format={:?}, raw={}",
            start_address,
            width,
            height,
            format,
            info.debug_raw_values()
        );

        // Copy texture data from the TexInfo
        let data = unsafe { info.get_data() }.to_vec();

        // Invalidate any existing Vulkan texture at this address (content may have changed)
        if let Some(app) = &mut self.vulkan_app {
            app.texture_cache
                .invalidate_texture(&app.device, start_address);
        }

        // Store texture in texture memory
        let texture = utility::texture::TextureData::new(width as u32, height as u32, format, data);
        self.texture_memory[tmu_idx].store_texture(start_address, texture);

        // Update current texture source
        self.state.tex_source[tmu_idx] = start_address;
    }

    /// grTexDownloadMipMapLevel - download a single mipmap level
    pub fn tex_download_mipmap_level(
        &mut self,
        tmu: ChipID,
        start_address: FxU32,
        this_lod: LOD,
        _large_lod: LOD,
        aspect_ratio: AspectRatio,
        format: TextureFormat,
        _even_odd: FxU32,
        data: *const std::ffi::c_void,
    ) {
        let tmu_idx = tmu as usize;
        if tmu_idx >= GLIDE_NUM_TMU {
            return;
        }

        let (width, height) = this_lod.to_texture_size(aspect_ratio);
        let bytes_per_pixel = format.bytes_per_pixel();
        let size = width * height * bytes_per_pixel;

        log::debug!(
            "Texture mipmap level download for TMU{}: addr={:#x}, {}x{}, lod={:?}, format={:?}",
            tmu_idx,
            start_address,
            width,
            height,
            this_lod,
            format
        );

        // Copy texture data from pointer
        let texture_data = if !data.is_null() {
            unsafe { std::slice::from_raw_parts(data as *const u8, size).to_vec() }
        } else {
            vec![0; size]
        };

        // Invalidate any existing Vulkan texture at this address
        if let Some(app) = &mut self.vulkan_app {
            app.texture_cache
                .invalidate_texture(&app.device, start_address);
        }

        // Store texture in texture memory
        let texture =
            utility::texture::TextureData::new(width as u32, height as u32, format, texture_data);
        self.texture_memory[tmu_idx].store_texture(start_address, texture);
    }

    /// grTexDownloadMipMapLevelPartial - download a partial mipmap level (row range)
    pub fn tex_download_mipmap_level_partial(
        &mut self,
        tmu: ChipID,
        start_address: FxU32,
        this_lod: LOD,
        _large_lod: LOD,
        aspect_ratio: AspectRatio,
        format: TextureFormat,
        _even_odd: FxU32,
        data: *const std::ffi::c_void,
        start_row: i32,
        end_row: i32,
    ) {
        let tmu_idx = tmu as usize;
        if tmu_idx >= GLIDE_NUM_TMU {
            return;
        }

        let (width, height) = this_lod.to_texture_size(aspect_ratio);
        let bytes_per_pixel = format.bytes_per_pixel();
        let row_bytes = width * bytes_per_pixel;

        // Clamp row range to valid values
        let start_row = start_row.max(0) as usize;
        let end_row = (end_row as usize).min(height);
        let num_rows = end_row.saturating_sub(start_row);

        if num_rows == 0 {
            return;
        }

        log::debug!(
            "Texture mipmap level partial download for TMU{}: addr={:#x}, {}x{}, rows {}..{}, format={:?}",
            tmu_idx,
            start_address,
            width,
            height,
            start_row,
            end_row,
            format
        );

        // Get or create texture at this address
        let texture = self.texture_memory[tmu_idx].get_texture_mut(start_address);

        if let Some(tex) = texture {
            // Update existing texture with partial data
            if !data.is_null() {
                let src_data =
                    unsafe { std::slice::from_raw_parts(data as *const u8, num_rows * row_bytes) };
                let dst_offset = start_row * row_bytes;
                let dst_end = dst_offset + num_rows * row_bytes;
                if dst_end <= tex.data.len() {
                    tex.data[dst_offset..dst_end].copy_from_slice(src_data);
                }
            }
        } else {
            // Create new texture and fill partial data
            let total_size = width * height * bytes_per_pixel;
            let mut texture_data = vec![0u8; total_size];

            if !data.is_null() {
                let src_data =
                    unsafe { std::slice::from_raw_parts(data as *const u8, num_rows * row_bytes) };
                let dst_offset = start_row * row_bytes;
                texture_data[dst_offset..dst_offset + num_rows * row_bytes]
                    .copy_from_slice(src_data);
            }

            let texture = utility::texture::TextureData::new(
                width as u32,
                height as u32,
                format,
                texture_data,
            );
            self.texture_memory[tmu_idx].store_texture(start_address, texture);
        }
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

        log::debug!("Texture table download: type={:?}", table_type);

        match table_type {
            TextureTableType::Palette | TextureTableType::Palette6666Ext => {
                // Copy palette data (256 entries * 4 bytes each = 1024 bytes)
                let palette_data =
                    unsafe { std::slice::from_raw_parts(data as *const u8, 256 * 4) };
                let palette = utility::texture::PaletteTable::from_data(palette_data);
                // Always download to TMU0
                self.texture_memory[0].store_palette(0, palette);
            }
            TextureTableType::Ncc0 | TextureTableType::Ncc1 => {
                // NCC (Narrow Channel Compression) tables are used for YIQ texture formats
                // Structure: 16 bytes yRGB + 24 bytes iRGB + 24 bytes qRGB + 48 bytes packed = 112 bytes
                // For now, store as raw data - NCC decompression would be done during texture sampling
                log::debug!(
                    "NCC table download (type {:?}) - not fully implemented",
                    table_type
                );
                // TODO: Implement NCC table storage and decompression
            }
        }

        // Store current table type in state
        self.state.tex_table_type = table_type;
    }

    // ==================== Drawing Functions ====================

    /// grDrawTriangle - draw a triangle
    pub fn draw_triangle(&mut self, a: &Vertex, b: &Vertex, c: &Vertex) {
        // Apply Y-flip for lower-left origin
        let (a, b, c) = if self.state.origin == OriginLocation::LowerLeft {
            let h = self.state.screen_height as f32;
            let mut a = *a;
            let mut b = *b;
            let mut c = *c;
            a.y = h - a.y;
            b.y = h - b.y;
            c.y = h - c.y;
            (a, b, c)
        } else {
            (*a, *b, *c)
        };

        // Check culling
        if self.state.cull_mode != CullMode::Disable {
            // Calculate signed area (cross product of edges)
            let area = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
            match self.state.cull_mode {
                CullMode::Negative if area < 0.0 => return,
                CullMode::Positive if area > 0.0 => return,
                _ => {}
            }
        }

        // Stamp per-vertex state (constant color + packed combine flags + texture index)
        let cc = self.current_constant_color();
        let cf = self.current_combine_flags();
        let tex_addr = self.state.tex_source[0];
        self.frame_textures.insert(tex_addr);
        // Store raw texture address; resolved to descriptor slot in buffer_swap after upload
        let tex_idx = tex_addr;
        let mut v0 = utility::vertex::GpuVertex::from_glide_vertex(&a);
        let mut v1 = utility::vertex::GpuVertex::from_glide_vertex(&b);
        let mut v2 = utility::vertex::GpuVertex::from_glide_vertex(&c);
        v0.constant_color = cc; v0.combine_flags = cf; v0.texture_index = tex_idx;
        v1.constant_color = cc; v1.combine_flags = cf; v1.texture_index = tex_idx;
        v2.constant_color = cc; v2.combine_flags = cf; v2.texture_index = tex_idx;

        if !self.vertex_batch.push_triangle(v0, v1, v2) {
            log::warn!("Vertex batch full, triangle dropped");
        }
    }

    /// grDrawPoint - draw a point as a small quad
    pub fn draw_point(&mut self, pt: &Vertex) {
        // Apply Y-flip for lower-left origin
        let pt = if self.state.origin == OriginLocation::LowerLeft {
            let h = self.state.screen_height as f32;
            let mut pt = *pt;
            pt.y = h - pt.y;
            pt
        } else {
            *pt
        };

        // Render point as a 1-pixel quad (2 triangles)
        const POINT_SIZE: f32 = 0.5;

        let mut gpu_v = utility::vertex::GpuVertex::from_glide_vertex(&pt);
        gpu_v.constant_color = self.current_constant_color();
        gpu_v.combine_flags = self.current_combine_flags();
        let tex_addr = self.state.tex_source[0];
        self.frame_textures.insert(tex_addr);
        gpu_v.texture_index = tex_addr;
        let x = gpu_v.position[0];
        let y = gpu_v.position[1];

        // Create 4 corners of a small quad
        let v0 = utility::vertex::GpuVertex {
            position: [x - POINT_SIZE, y - POINT_SIZE],
            ..gpu_v
        };
        let v1 = utility::vertex::GpuVertex {
            position: [x + POINT_SIZE, y - POINT_SIZE],
            ..gpu_v
        };
        let v2 = utility::vertex::GpuVertex {
            position: [x + POINT_SIZE, y + POINT_SIZE],
            ..gpu_v
        };
        let v3 = utility::vertex::GpuVertex {
            position: [x - POINT_SIZE, y + POINT_SIZE],
            ..gpu_v
        };

        // Two triangles to form the quad
        if !self.vertex_batch.push_triangle(v0, v1, v2) {
            log::warn!("Vertex batch full, point dropped");
            return;
        }
        if !self.vertex_batch.push_triangle(v0, v2, v3) {
            log::warn!("Vertex batch full, point dropped");
        }
    }

    /// grDrawLine - draw a line as a thin quad
    pub fn draw_line(&mut self, v1: &Vertex, v2: &Vertex) {
        // Apply Y-flip for lower-left origin
        let (v1, v2) = if self.state.origin == OriginLocation::LowerLeft {
            let h = self.state.screen_height as f32;
            let mut v1 = *v1;
            let mut v2 = *v2;
            v1.y = h - v1.y;
            v2.y = h - v2.y;
            (v1, v2)
        } else {
            (*v1, *v2)
        };

        // Render line as a thin quad (2 triangles)
        const LINE_WIDTH: f32 = 0.5;

        let cc = self.current_constant_color();
        let cf = self.current_combine_flags();
        let tex_addr = self.state.tex_source[0];
        self.frame_textures.insert(tex_addr);
        // Store raw texture address; resolved to descriptor slot in buffer_swap after upload
        let tex_idx = tex_addr;
        let mut gpu_v1 = utility::vertex::GpuVertex::from_glide_vertex(&v1);
        let mut gpu_v2 = utility::vertex::GpuVertex::from_glide_vertex(&v2);
        gpu_v1.constant_color = cc; gpu_v1.combine_flags = cf; gpu_v1.texture_index = tex_idx;
        gpu_v2.constant_color = cc; gpu_v2.combine_flags = cf; gpu_v2.texture_index = tex_idx;

        let x1 = gpu_v1.position[0];
        let y1 = gpu_v1.position[1];
        let x2 = gpu_v2.position[0];
        let y2 = gpu_v2.position[1];

        // Calculate perpendicular direction for line width
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt();

        if len < 0.001 {
            // Line too short, draw as a point
            self.draw_point(&v1);
            return;
        }

        // Normalized perpendicular
        let px = -dy / len * LINE_WIDTH;
        let py = dx / len * LINE_WIDTH;

        // Create 4 corners of the line quad
        let c0 = utility::vertex::GpuVertex {
            position: [x1 - px, y1 - py],
            ..gpu_v1
        };
        let c1 = utility::vertex::GpuVertex {
            position: [x1 + px, y1 + py],
            ..gpu_v1
        };
        let c2 = utility::vertex::GpuVertex {
            position: [x2 + px, y2 + py],
            ..gpu_v2
        };
        let c3 = utility::vertex::GpuVertex {
            position: [x2 - px, y2 - py],
            ..gpu_v2
        };

        // Two triangles to form the quad
        if !self.vertex_batch.push_triangle(c0, c1, c2) {
            log::warn!("Vertex batch full, line dropped");
            return;
        }
        if !self.vertex_batch.push_triangle(c0, c2, c3) {
            log::warn!("Vertex batch full, line dropped");
        }
    }

    /// grDrawVertexArray - draw primitives from an array of vertex pointers
    pub unsafe fn draw_vertex_array(
        &mut self,
        mode: PrimitiveType,
        count: FxU32,
        pointers: *const *const Vertex,
    ) {
        if pointers.is_null() || count == 0 {
            return;
        }

        let layout = &self.state.vertex_layout;
        let origin = self.state.origin;
        let screen_height = self.state.screen_height as f32;
        let cull_mode = self.state.cull_mode;
        let vertex_size = layout.vertex_size();
        let cc = self.current_constant_color();
        let cf = self.current_combine_flags();
        let tex_addr = self.state.tex_source[0];
        self.frame_textures.insert(tex_addr);
        // Store raw texture address; resolved to descriptor slot in buffer_swap after upload
        let tex_idx = tex_addr;

        // Extract vertices using the configured layout
        let gpu_vertices: Vec<utility::vertex::GpuVertex> = (0..count as usize)
            .filter_map(|i| unsafe {
                let ptr = *pointers.add(i);
                if ptr.is_null() {
                    return None;
                }

                let vertex_data = std::slice::from_raw_parts(ptr as *const u8, vertex_size);
                let (x, mut y, r, g, b, a, s, t, oow) = layout.extract_vertex(vertex_data);

                // Apply Y-flip for lower-left origin
                if origin == OriginLocation::LowerLeft {
                    y = screen_height - y;
                }

                let mut v = utility::vertex::GpuVertex::from_layout_data(
                    x, y, r, g, b, a, s, t, oow,
                );
                v.constant_color = cc;
                v.combine_flags = cf;
                v.texture_index = tex_idx;
                Some(v)
            })
            .collect();

        // Draw primitives based on mode
        self.draw_primitives_gpu(mode, &gpu_vertices, cull_mode);
    }

    /// grDrawVertexArrayContiguous - draw primitives from a contiguous vertex array
    pub fn draw_vertex_array_contiguous(
        &mut self,
        mode: PrimitiveType,
        count: FxU32,
        pointers: *const Vertex,
        stride: FxU32,
    ) {
        if pointers.is_null() || count == 0 {
            return;
        }

        let layout = &self.state.vertex_layout;
        let origin = self.state.origin;
        let screen_height = self.state.screen_height as f32;
        let cull_mode = self.state.cull_mode;
        let current_texture = self.state.tex_source[0];
        self.frame_textures.insert(current_texture);
        let cc = self.current_constant_color();
        let cf = self.current_combine_flags();
        let tex_idx = current_texture;

        // Log small texture draws (likely glyphs) for debugging
        static LOG_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let draw_num = LOG_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Get texture dimensions to identify glyph draws
        let tex_size = self.texture_memory[0]
            .get_texture(current_texture)
            .map(|t| (t.width, t.height))
            .unwrap_or((0, 0));

        // Log first 20 small texture draws (32x32 or smaller = likely glyphs)
        let is_small_texture = tex_size.0 <= 32 && tex_size.1 <= 32 && tex_size.0 > 0;
        let should_log = is_small_texture && draw_num < 20;

        if is_small_texture && should_log {
            log::trace!(
                "grDrawVertexArrayContiguous: texture={:#x} ({}x{}), mode={:?}, count={}",
                current_texture,
                tex_size.0,
                tex_size.1,
                mode,
                count
            );
        }

        // Extract vertices using the configured layout
        let gpu_vertices: Vec<utility::vertex::GpuVertex> = (0..count as usize)
            .map(|i| unsafe {
                let byte_ptr = (pointers as *const u8).add(i * stride as usize);
                let vertex_data = std::slice::from_raw_parts(byte_ptr, stride as usize);

                let (x, mut y, r, g, b, a, s, t, oow) = layout.extract_vertex(vertex_data);

                // Log first 4 vertices of small texture draws at trace level
                if should_log && i < 4 {
                    log::trace!(
                        "  Vertex {}: pos=({:.1}, {:.1}), st=({:.1}, {:.1})",
                        i,
                        x,
                        y,
                        s,
                        t
                    );
                }

                // Apply Y-flip for lower-left origin
                if origin == OriginLocation::LowerLeft {
                    y = screen_height - y;
                }

                let mut v = utility::vertex::GpuVertex::from_layout_data(x, y, r, g, b, a, s, t, oow);
                v.constant_color = cc;
                v.combine_flags = cf;
                v.texture_index = tex_idx;
                v
            })
            .collect();

        // Draw primitives based on mode
        self.draw_primitives_gpu(mode, &gpu_vertices, cull_mode);
    }

    /// Internal helper to draw primitives from GPU vertices
    fn draw_primitives_gpu(
        &mut self,
        mode: PrimitiveType,
        vertices: &[utility::vertex::GpuVertex],
        cull_mode: CullMode,
    ) {
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
                    for i in 1..vertices.len() - 1 {
                        self.draw_triangle_gpu(first, vertices[i], vertices[i + 1], cull_mode);
                    }
                }
            }
            PrimitiveType::TriangleStrip => {
                for i in 0..vertices.len().saturating_sub(2) {
                    if i % 2 == 0 {
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
                    for i in 1..vertices.len() - 1 {
                        self.draw_triangle_gpu(first, vertices[i], vertices[i + 1], cull_mode);
                    }
                }
            }
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

    /// Internal helper to draw primitives based on mode
    fn draw_primitives(&mut self, mode: PrimitiveType, vertices: &[&Vertex]) {
        match mode {
            PrimitiveType::Points => {
                for v in vertices {
                    self.draw_point(v);
                }
            }
            PrimitiveType::Lines => {
                // Draw pairs of vertices as lines
                for chunk in vertices.chunks(2) {
                    if chunk.len() == 2 {
                        self.draw_line(chunk[0], chunk[1]);
                    }
                }
            }
            PrimitiveType::LineStrip => {
                // Draw connected lines
                for window in vertices.windows(2) {
                    self.draw_line(window[0], window[1]);
                }
            }
            PrimitiveType::Triangles => {
                // Draw groups of 3 vertices as triangles
                for chunk in vertices.chunks(3) {
                    if chunk.len() == 3 {
                        self.draw_triangle(chunk[0], chunk[1], chunk[2]);
                    }
                }
            }
            PrimitiveType::TriangleStrip => {
                // Draw triangle strip (v0,v1,v2), (v2,v1,v3), (v2,v3,v4)...
                for i in 0..vertices.len().saturating_sub(2) {
                    if i % 2 == 0 {
                        self.draw_triangle(vertices[i], vertices[i + 1], vertices[i + 2]);
                    } else {
                        self.draw_triangle(vertices[i + 1], vertices[i], vertices[i + 2]);
                    }
                }
            }
            PrimitiveType::TriangleFan => {
                // Draw triangle fan: all triangles share first vertex
                if vertices.len() >= 3 {
                    let first = vertices[0];
                    for i in 1..vertices.len() - 1 {
                        self.draw_triangle(first, vertices[i], vertices[i + 1]);
                    }
                }
            }
            PrimitiveType::Polygon => {
                // Draw polygon as triangle fan (same as fan for convex polygons)
                if vertices.len() >= 3 {
                    let first = vertices[0];
                    for i in 1..vertices.len() - 1 {
                        self.draw_triangle(first, vertices[i], vertices[i + 1]);
                    }
                }
            }
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

    /// Read the current framebuffer contents
    /// Returns (width, height, pixel_data) where pixel_data is BGRA bytes (4 bytes per pixel)
    pub fn lfb_read_region(&self) -> Option<(u32, u32, Vec<u8>)> {
        let app = self.vulkan_app.as_ref()?;

        unsafe {
            // Wait for rendering to complete
            app.device
                .device_wait_idle()
                .expect("Failed to wait for device idle");

            let width = app.swapchain_extent.width;
            let height = app.swapchain_extent.height;
            let image_size = (width * height * 4) as vk::DeviceSize;

            // Create staging buffer for readback
            let buffer_info = vk::BufferCreateInfo::default()
                .size(image_size)
                .usage(vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let staging_buffer = app
                .device
                .create_buffer(&buffer_info, None)
                .expect("Failed to create staging buffer");

            let mem_requirements = app.device.get_buffer_memory_requirements(staging_buffer);

            // Find host-visible memory type
            let instance = &self.vulkan_instance.instance;
            let mem_properties =
                instance.get_physical_device_memory_properties(app.physical_device);

            let mut memory_type_index = 0;
            for i in 0..mem_properties.memory_type_count {
                if (mem_requirements.memory_type_bits & (1 << i)) != 0
                    && mem_properties.memory_types[i as usize]
                        .property_flags
                        .contains(
                            vk::MemoryPropertyFlags::HOST_VISIBLE
                                | vk::MemoryPropertyFlags::HOST_COHERENT,
                        )
                {
                    memory_type_index = i;
                    break;
                }
            }

            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(mem_requirements.size)
                .memory_type_index(memory_type_index);

            let staging_memory = app
                .device
                .allocate_memory(&alloc_info, None)
                .expect("Failed to allocate staging memory");

            app.device
                .bind_buffer_memory(staging_buffer, staging_memory, 0)
                .expect("Failed to bind staging buffer memory");

            // Create command buffer for copy
            let cmd_alloc_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(app.command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);

            let cmd_buffers = app
                .device
                .allocate_command_buffers(&cmd_alloc_info)
                .expect("Failed to allocate command buffer");
            let cmd = cmd_buffers[0];

            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

            app.device
                .begin_command_buffer(cmd, &begin_info)
                .expect("Failed to begin command buffer");

            // Read from the last presented image (front buffer)
            log::debug!(
                "lfb_read_region: reading from image index {} (last_presented={}, current={})",
                app.last_presented_image_index, app.last_presented_image_index, app.current_image_index
            );
            let src_image = app.swapchain_images[app.last_presented_image_index as usize];

            let barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(src_image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::MEMORY_READ)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ);

            app.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );

            // Copy image to buffer
            let region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                });

            app.device.cmd_copy_image_to_buffer(
                cmd,
                src_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                staging_buffer,
                &[region],
            );

            // Transition image back
            let barrier_back = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(src_image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ);

            app.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier_back],
            );

            app.device
                .end_command_buffer(cmd)
                .expect("Failed to end command buffer");

            // Submit and wait
            let submit_info = vk::SubmitInfo::default().command_buffers(&cmd_buffers);

            app.device
                .queue_submit(app.graphics_queue, &[submit_info], vk::Fence::null())
                .expect("Failed to submit copy command");

            app.device
                .queue_wait_idle(app.graphics_queue)
                .expect("Failed to wait for queue");

            // Read back data
            let data_ptr = app
                .device
                .map_memory(staging_memory, 0, image_size, vk::MemoryMapFlags::empty())
                .expect("Failed to map memory");

            let mut pixels = vec![0u8; image_size as usize];
            std::ptr::copy_nonoverlapping(
                data_ptr as *const u8,
                pixels.as_mut_ptr(),
                image_size as usize,
            );

            app.device.unmap_memory(staging_memory);

            // Cleanup
            app.device
                .free_command_buffers(app.command_pool, &cmd_buffers);
            app.device.destroy_buffer(staging_buffer, None);
            app.device.free_memory(staging_memory, None);

            Some((width, height, pixels))
        }
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
        &self,
        _src_buffer: glidesys::Buffer,
        src_x: FxU32,
        src_y: FxU32,
        src_width: FxU32,
        src_height: FxU32,
        dst_stride: FxU32,
        dst_data: *mut std::ffi::c_void,
    ) -> glidesys::FxBool {
        // Read entire framebuffer
        if let Some((fb_width, fb_height, pixels)) = self.lfb_read_region() {
            // Validate bounds
            if src_x + src_width > fb_width || src_y + src_height > fb_height {
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
                        *dst_ptr = rgb565;
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
        // Check if already locked
        if self.lfb_lock_state.is_locked {
            log::warn!("LFB already locked");
            return FxBool::False;
        }

        let app = match self.vulkan_app.as_ref() {
            Some(app) => app,
            None => return FxBool::False,
        };

        let width = app.swapchain_extent.width;
        let height = app.swapchain_extent.height;

        // Calculate stride based on write mode (bytes per pixel)
        let bytes_per_pixel = match write_mode {
            glidesys::LfbWriteMode::Rgb565
            | glidesys::LfbWriteMode::Rgb555
            | glidesys::LfbWriteMode::Argb1555 => 2,
            glidesys::LfbWriteMode::Rgb888 => 3,
            glidesys::LfbWriteMode::Argb8888 => 4,
            glidesys::LfbWriteMode::Rgb565Depth
            | glidesys::LfbWriteMode::Rgb555Depth
            | glidesys::LfbWriteMode::Argb1555Depth
            | glidesys::LfbWriteMode::Za16 => 4,
            _ => 2, // Default to 16-bit
        };
        let stride = width * bytes_per_pixel;

        // Allocate buffer
        let buffer_size = (stride * height) as usize;
        let mut data = vec![0u8; buffer_size];

        // For read locks, copy framebuffer data to buffer
        if lock_type == glidesys::LfbLock::ReadOnly {
            if let Some((_, _, pixels)) = self.lfb_read_region() {
                // Convert from BGRA to requested format
                match write_mode {
                    glidesys::LfbWriteMode::Rgb565 | glidesys::LfbWriteMode::Any => {
                        // Convert BGRA to RGB565
                        for y in 0..height {
                            for x in 0..width {
                                let src_idx = ((y * width + x) * 4) as usize;
                                let dst_idx = ((y * stride) + (x * 2)) as usize;
                                if src_idx + 2 < pixels.len() && dst_idx + 1 < data.len() {
                                    let b = pixels[src_idx] as u16;
                                    let g = pixels[src_idx + 1] as u16;
                                    let r = pixels[src_idx + 2] as u16;
                                    let rgb565: u16 = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
                                    data[dst_idx] = (rgb565 & 0xFF) as u8;
                                    data[dst_idx + 1] = ((rgb565 >> 8) & 0xFF) as u8;
                                }
                            }
                        }
                    }
                    _ => {
                        // For other formats, just copy raw BGRA data
                        let copy_len = std::cmp::min(data.len(), pixels.len());
                        data[..copy_len].copy_from_slice(&pixels[..copy_len]);
                    }
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
        if !self.lfb_lock_state.is_locked {
            return FxBool::False;
        }

        if self.lfb_lock_state.lock_type != lock_type || self.lfb_lock_state.buffer != buffer {
            return FxBool::False;
        }

        // For write locks, upload data directly to swapchain image and present
        if lock_type == glidesys::LfbLock::WriteOnly {
            self.lfb_upload_to_framebuffer();
            self.lfb_lock_state.lfb_written = true;
        }

        // Clear lock state
        self.lfb_lock_state.is_locked = false;
        self.lfb_lock_state.data.clear();

        FxBool::True
    }

    /// grLfbWriteRegion - write a pixel rectangle to the frame buffer
    pub fn lfb_write_region(
        &mut self,
        dst_buffer: glidesys::Buffer,
        dst_x: FxU32,
        dst_y: FxU32,
        src_format: glidesys::LfbWriteMode,
        src_width: FxU32,
        src_height: FxU32,
        _pixel_pipeline: FxBool,
        src_stride: i32,
        src_data: *const std::ffi::c_void,
    ) -> FxBool {
        if src_data.is_null() || src_width == 0 || src_height == 0 {
            return FxBool::False;
        }

        // Lock the LFB
        let mut info = glidesys::LfbInfo {
            size: std::mem::size_of::<glidesys::LfbInfo>() as i32,
            lfb_ptr: std::ptr::null_mut(),
            stride_in_bytes: 0,
            write_mode: glidesys::LfbWriteMode::Any,
            origin: glidesys::OriginLocation::UpperLeft,
        };

        if self.lfb_lock(
            glidesys::LfbLock::WriteOnly,
            dst_buffer,
            src_format,
            glidesys::OriginLocation::UpperLeft,
            FxBool::False,
            &mut info,
        ) != FxBool::True
        {
            return FxBool::False;
        }

        // Calculate bytes per pixel
        let bpp: u32 = match src_format {
            glidesys::LfbWriteMode::Rgb565
            | glidesys::LfbWriteMode::Rgb555
            | glidesys::LfbWriteMode::Argb1555 => 2,
            glidesys::LfbWriteMode::Rgb888 => 3,
            glidesys::LfbWriteMode::Argb8888 => 4,
            _ => 2,
        };

        // Copy row by row from source to locked buffer
        let dst_stride = info.stride_in_bytes;
        unsafe {
            let src_ptr = src_data as *const u8;
            let dst_ptr = info.lfb_ptr as *mut u8;
            for y in 0..src_height {
                let src_row = src_ptr.add((y as i32 * src_stride) as usize);
                let dst_row = dst_ptr.add(((dst_y + y) * dst_stride + dst_x * bpp) as usize);
                std::ptr::copy_nonoverlapping(src_row, dst_row, (src_width * bpp) as usize);
            }
        }

        // Unlock triggers upload
        self.lfb_unlock(glidesys::LfbLock::WriteOnly, dst_buffer)
    }

    /// Upload LFB write data directly to the swapchain image and present.
    /// Uses vkCmdCopyBufferToImage to bypass the shader pipeline entirely.
    fn lfb_upload_to_framebuffer(&mut self) {
        let app = match self.vulkan_app.as_mut() {
            Some(app) => app,
            None => return,
        };

        let width = self.lfb_lock_state.width;
        let height = self.lfb_lock_state.height;
        let stride = self.lfb_lock_state.stride;
        let write_mode = self.lfb_lock_state.write_mode;
        let origin = self.lfb_lock_state.origin;
        let src_data = &self.lfb_lock_state.data;

        let frame = app.current_frame;
        unsafe {
            app.device
                .queue_wait_idle(app.graphics_queue)
                .expect("Failed to wait for graphics queue");
            app.device
                .wait_for_fences(&[app.in_flight_fences[frame]], true, 0)
                .ok();
            app.device
                .reset_fences(&[app.in_flight_fences[frame]])
                .expect("Failed to reset fence before acquire");

            loop {
                match app.swapchain_loader.acquire_next_image(
                    app.swapchain,
                    1_000_000_000,
                    vk::Semaphore::null(),
                    app.in_flight_fences[frame],
                ) {
                    Ok((image_index, _)) => {
                        app.current_image_index = image_index;
                        app.device
                            .wait_for_fences(&[app.in_flight_fences[frame]], true, u64::MAX)
                            .expect("Failed to wait for acquire fence");
                        app.device
                            .reset_fences(&[app.in_flight_fences[frame]])
                            .expect("Failed to reset fence after acquire");
                        break;
                    }
                    Err(vk::Result::NOT_READY | vk::Result::TIMEOUT) => {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        continue;
                    }
                    Err(e) => panic!("Failed to acquire next image: {:?}", e),
                }
            }
        }

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
                                let rgb565 = (src_data[src_idx] as u16)
                                    | ((src_data[src_idx + 1] as u16) << 8);
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
                                let rgb555 = (src_data[src_idx] as u16)
                                    | ((src_data[src_idx + 1] as u16) << 8);
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
                                let argb1555 = (src_data[src_idx] as u16)
                                    | ((src_data[src_idx + 1] as u16) << 8);
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
                                (src_data[src_idx + 2], src_data[src_idx + 1], src_data[src_idx], 255u8)
                            } else {
                                (0, 0, 0, 255)
                            }
                        }
                        glidesys::LfbWriteMode::Argb8888 => {
                            if src_idx + 3 < src_data.len() {
                                (src_data[src_idx + 2], src_data[src_idx + 1], src_data[src_idx], src_data[src_idx + 3])
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

        // Copy BGRA data directly to swapchain image via staging buffer
        unsafe {
            let image_size = bgra_data.len() as vk::DeviceSize;

            let buffer_info = vk::BufferCreateInfo::default()
                .size(image_size)
                .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let staging_buffer = app
                .device
                .create_buffer(&buffer_info, None)
                .expect("Failed to create staging buffer");

            let mem_requirements = app.device.get_buffer_memory_requirements(staging_buffer);

            let instance = &self.vulkan_instance.instance;
            let mem_properties =
                instance.get_physical_device_memory_properties(app.physical_device);

            let mut memory_type_index = 0;
            for i in 0..mem_properties.memory_type_count {
                if (mem_requirements.memory_type_bits & (1 << i)) != 0
                    && mem_properties.memory_types[i as usize]
                        .property_flags
                        .contains(
                            vk::MemoryPropertyFlags::HOST_VISIBLE
                                | vk::MemoryPropertyFlags::HOST_COHERENT,
                        )
                {
                    memory_type_index = i;
                    break;
                }
            }

            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(mem_requirements.size)
                .memory_type_index(memory_type_index);

            let staging_memory = app
                .device
                .allocate_memory(&alloc_info, None)
                .expect("Failed to allocate staging memory");

            app.device
                .bind_buffer_memory(staging_buffer, staging_memory, 0)
                .expect("Failed to bind staging buffer memory");

            let data_ptr = app
                .device
                .map_memory(staging_memory, 0, image_size, vk::MemoryMapFlags::empty())
                .expect("Failed to map memory");

            std::ptr::copy_nonoverlapping(bgra_data.as_ptr(), data_ptr as *mut u8, bgra_data.len());

            app.device.unmap_memory(staging_memory);

            // Record command buffer: transition → copy → transition → present
            let cmd_alloc_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(app.command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);

            let cmd_buffers = app
                .device
                .allocate_command_buffers(&cmd_alloc_info)
                .expect("Failed to allocate command buffer");
            let cmd = cmd_buffers[0];

            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

            app.device.begin_command_buffer(cmd, &begin_info).expect("Failed to begin command buffer");

            let dst_image = app.swapchain_images[app.current_image_index as usize];

            // Transition to TRANSFER_DST
            let barrier = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(dst_image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0, level_count: 1,
                    base_array_layer: 0, layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);

            app.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[], &[], &[barrier],
            );

            // Copy buffer to image
            let region = vk::BufferImageCopy::default()
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0, base_array_layer: 0, layer_count: 1,
                })
                .image_extent(vk::Extent3D { width, height, depth: 1 });

            app.device.cmd_copy_buffer_to_image(
                cmd, staging_buffer, dst_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[region],
            );

            // Transition to PRESENT_SRC
            let barrier_present = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(dst_image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0, level_count: 1,
                    base_array_layer: 0, layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ);

            app.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[], &[], &[barrier_present],
            );

            app.device.end_command_buffer(cmd).expect("Failed to end command buffer");

            // Submit and wait
            let submit_info = vk::SubmitInfo::default().command_buffers(&cmd_buffers);
            app.device
                .queue_submit(app.graphics_queue, &[submit_info], vk::Fence::null())
                .expect("Failed to submit copy command");

            app.device.queue_wait_idle(app.graphics_queue).expect("Failed to wait for queue");

            // Present
            let swapchains = [app.swapchain];
            let image_indices = [app.current_image_index];
            let present_info = vk::PresentInfoKHR::default()
                .swapchains(&swapchains)
                .image_indices(&image_indices);

            let _ = app.swapchain_loader.queue_present(app.graphics_queue, &present_info);

            app.last_presented_image_index = app.current_image_index;
            log::debug!(
                "lfb_upload: set last_presented={} (current={})",
                app.last_presented_image_index, app.current_image_index
            );

            // Signal fence so next buffer_swap won't hang
            let empty_submit = vk::SubmitInfo::default();
            app.device
                .queue_submit(app.graphics_queue, &[empty_submit], app.in_flight_fences[frame])
                .expect("Failed to signal fence");

            // Cleanup
            app.device.free_command_buffers(app.command_pool, &cmd_buffers);
            app.device.destroy_buffer(staging_buffer, None);
            app.device.free_memory(staging_memory, None);

            log::debug!(
                "LFB write uploaded to framebuffer ({}x{}, image_index={})",
                width, height, app.current_image_index
            );
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

    // ==================== grGetString Tests ====================

    #[test]
    fn test_get_string_renderer() {
        let mut gr = create_test_gr();
        let ptr = gr.get_string(EnvironmentInfo::Renderer);
        let s = unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str().unwrap();
        assert_eq!(s, "Glide");
    }

    #[test]
    fn test_get_string_vendor() {
        let mut gr = create_test_gr();
        let ptr = gr.get_string(EnvironmentInfo::Vendor);
        let s = unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str().unwrap();
        assert_eq!(s, "3Dfx Interactive");
    }

    #[test]
    fn test_get_string_version() {
        let mut gr = create_test_gr();
        let ptr = gr.get_string(EnvironmentInfo::Version);
        let s = unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str().unwrap();
        assert_eq!(s, "3.0");
    }

    #[test]
    fn test_get_string_hardware() {
        let mut gr = create_test_gr();
        let ptr = gr.get_string(EnvironmentInfo::Hardware);
        let s = unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str().unwrap();
        assert_eq!(s, "Banshee");
    }

    #[test]
    fn test_get_string_extension() {
        let mut gr = create_test_gr();
        let ptr = gr.get_string(EnvironmentInfo::Extension);
        let s = unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str().unwrap();
        assert_eq!(s, " "); // Just a space
    }

    // ==================== grGet Tests ====================

    #[test]
    fn test_get_num_boards() {
        let mut gr = create_test_gr();
        let mut value: i32 = 0;
        let result = gr.get(EnvironmentParameters::NumBoards, 4, &mut value);
        assert_eq!(result, 1);
        assert_eq!(value, 1);
    }

    #[test]
    fn test_get_max_texture_size() {
        let mut gr = create_test_gr();
        let mut value: i32 = 0;
        let result = gr.get(EnvironmentParameters::MaxTextureSize, 4, &mut value);
        assert_eq!(result, 1);
        assert_eq!(value, 256);
    }

    #[test]
    fn test_get_num_fb() {
        let mut gr = create_test_gr();
        let mut value: i32 = 0;
        let result = gr.get(EnvironmentParameters::NumFb, 4, &mut value);
        assert_eq!(result, 1);
        assert_eq!(value, 1);
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
            TextureCombineFnc::Decal,
            CombineFactor::One,
            TextureCombineFnc::Decal,
            CombineFactor::One,
            FxBool::False,
            FxBool::False,
        );
        assert!(matches!(
            gr.state.tex_combine_function[0],
            TextureCombineFnc::Decal
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
        assert!(matches!(gr.state.color_combine_local, CombineLocal::Constant));
        assert!(matches!(gr.state.color_combine_other, CombineOther::Iterated));
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
        assert!(matches!(gr.state.alpha_combine_factor, CombineFactor::Local));
        assert!(matches!(gr.state.alpha_combine_local, CombineLocal::Iterated));
        assert!(matches!(gr.state.alpha_combine_other, CombineOther::Texture));
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
