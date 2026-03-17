#![allow(non_snake_case, dead_code)]

use lazy_static::lazy_static;
use log::{debug, trace};
use std::sync::{Arc, Mutex};

use glidevk::Gr;

// Re-export commonly used types for test convenience (also brings them into scope)
pub use glidevk::glidesys::{
    Alpha, AlphaBlendFnc, AspectRatio, Buffer, ChipID, ChromakeyMode, CmpFnc, Color, ColorFormat,
    CombineFactor, CombineFunction, CombineLocal, CombineOther, CoordinateSpaceMode, CullMode,
    DepthBufferMode, DitherMode, EnableMode, EnvironmentInfo, FogMode, FxBool, FxI32, FxU32,
    GrContext, GrGetParam, LfbInfo, LfbLock, LfbWriteMode, MipMapMode, OriginLocation,
    PrimitiveType, ScreenRefresh, ScreenResolution, TexInfo, TextureClampMode, TextureCombineFnc,
    TextureFilterMode, TextureFormat, TextureTableType, Vertex, LOD,
};

// Re-export GrProc type from api module
pub use glidevk::api::GrProc;

pub mod testutil;

// Use "system" calling convention: stdcall on 32-bit Windows, C elsewhere
// This automatically exports with _name@N decoration on Win32

// macros for generating C bindings
macro_rules! define_function {
    ($func:ident => $api:ident($($arg:ident: $arg_type:ty),*)) => {
        define_function!($func => $api($($arg: $arg_type),*) -> ());
    };
    ($func:ident => $api:ident($($arg:ident: $arg_type:ty),*), $debug:expr) => {
        define_function!($func => $api($($arg: $arg_type),*) -> (), $debug);
    };
    ($func:ident => $api:ident($($arg:ident: $arg_type:ty),*) -> Option<$type:ty>) => {
        #[no_mangle]
        pub extern "system" fn $api($($arg: $arg_type),*) -> $type {
            let mut gr = GR.lock().unwrap();
            if let Some(gr) = (*gr).as_mut() {
                if let Some(result) = gr.$func($($arg),*) {
                    debug!(concat!(stringify!($api), "(", $(stringify!($arg), "={:?}, "),*, ")"), $($arg),*);
                    result
                } else {
                    debug!(concat!(stringify!($api), "(", $(stringify!($arg), "={:?}, "),*, ") returned None"), $($arg),*);
                    Default::default()
                }
            } else {
                debug!(concat!(stringify!($api), "() called before grGlideInit!"));
                Default::default()
            }
        }
    };
    ($func:ident => $api:ident($($arg:ident: $arg_type:ty),*) -> $type:ty) => {
        define_function!($func => $api($($arg: $arg_type),*) -> $type, concat!($(stringify!($arg), "={:?}, "),*));
    };
    ($func:ident => $api:ident($($arg:ident: $arg_type:ty),*) -> $type:ty, $debug:expr) => {
        #[no_mangle]
        pub extern "system" fn $api($($arg: $arg_type),*) -> $type {
            let mut gr = GR.lock().unwrap();
            if let Some(gr) = (*gr).as_mut() {
                debug!(concat!(stringify!($api), "(", $debug, ")"), $($arg),*);
                gr.$func($($arg),*)
            } else {
                debug!(concat!(stringify!($api), "() called before grGlideInit!"));
                Default::default()
            }
        }
    };
}

// Macro for functions that take vertex pointers (needs unsafe dereference)
macro_rules! define_draw_function {
    ($func:ident => $api:ident($($arg:ident: &Vertex),+)) => {
        #[no_mangle]
        pub extern "system" fn $api($($arg: *const Vertex),+) {
            let mut gr = GR.lock().unwrap();
            if let Some(gr) = (*gr).as_mut() {
                unsafe {
                    gr.$func($(&*$arg),+);
                }
            } else {
                debug!(concat!(stringify!($api), "() called before grGlideInit!"));
            }
        }
    };
}

// Macro for functions that take a pointer as the last argument (needs unsafe dereference)
macro_rules! define_ptr_function {
    // Pattern: regular args followed by one pointer arg that needs dereference
    ($func:ident => $api:ident($($arg:ident: $arg_type:ty),* ; $ptr:ident: &$ptr_type:ty)) => {
        #[no_mangle]
        pub extern "system" fn $api($($arg: $arg_type,)* $ptr: *const $ptr_type) {
            let mut gr = GR.lock().unwrap();
            if let Some(gr) = (*gr).as_mut() {
                trace!(concat!(stringify!($api), "(", $(stringify!($arg), "={:?}, ",)+ ")"), $($arg),+);
                unsafe {
                    gr.$func($($arg,)* &*$ptr);
                }
            } else {
                debug!(concat!(stringify!($api), "() called before grGlideInit!"));
            }
        }
    };
    // Pattern with return type
    ($func:ident => $api:ident($($arg:ident: $arg_type:ty),* ; $ptr:ident: &$ptr_type:ty) -> $ret:ty) => {
        #[no_mangle]
        pub extern "system" fn $api($($arg: $arg_type,)* $ptr: *const $ptr_type) -> $ret {
            let gr = GR.lock().unwrap();
            if let Some(gr) = (*gr).as_ref() {
                trace!(concat!(stringify!($api), "(", $(stringify!($arg), "={:?}, ",)+ ")"), $($arg),+);
                unsafe {
                    gr.$func($($arg,)* &*$ptr)
                }
            } else {
                debug!(concat!(stringify!($api), "() called before grGlideInit!"));
                Default::default()
            }
        }
    };
    // Pattern: mutable pointer with return type
    ($func:ident => $api:ident($($arg:ident: $arg_type:ty),* ; $ptr:ident: &mut $ptr_type:ty) -> $ret:ty) => {
        #[no_mangle]
        pub extern "system" fn $api($($arg: $arg_type,)* $ptr: *mut $ptr_type) -> $ret {
            let mut gr = GR.lock().unwrap();
            if let Some(gr) = (*gr).as_mut() {
                trace!(concat!(stringify!($api), "(", $(stringify!($arg), "={:?}, ",)+ ")"), $($arg),+);
                unsafe {
                    gr.$func($($arg,)* &mut *$ptr)
                }
            } else {
                debug!(concat!(stringify!($api), "() called before grGlideInit!"));
                Default::default()
            }
        }
    };
    // Pattern: raw pointer passed directly (no dereference)
    ($func:ident => $api:ident($($arg:ident: $arg_type:ty),* ; $ptr:ident: *mut $ptr_type:ty) -> $ret:ty) => {
        #[no_mangle]
        pub extern "system" fn $api($($arg: $arg_type,)* $ptr: *mut $ptr_type) -> $ret {
            let gr = GR.lock().unwrap();
            if let Some(gr) = (*gr).as_ref() {
                trace!(concat!(stringify!($api), "(", $(stringify!($arg), "={:?}, ",)+ ")"), $($arg),+);
                gr.$func($($arg,)* $ptr)
            } else {
                debug!(concat!(stringify!($api), "() called before grGlideInit!"));
                Default::default()
            }
        }
    };
}

// Macro for vertex array drawing functions (pointer-to-pointers)
macro_rules! define_vertex_array_function {
    // Pattern: mode, count, and pointer-to-pointers
    ($func:ident => $api:ident(mode: PrimitiveType, count: FxU32, pointers: *const *const Vertex)) => {
        #[no_mangle]
        pub extern "system" fn $api(
            mode: PrimitiveType,
            count: FxU32,
            pointers: *const *const Vertex,
        ) {
            let mut gr = GR.lock().unwrap();
            if let Some(gr) = (*gr).as_mut() {
                debug!(
                    concat!(stringify!($api), "(mode={:?}, count={:?})"),
                    mode, count
                );
                gr.$func(mode, count, pointers);
            } else {
                debug!(concat!(stringify!($api), "() called before grGlideInit!"));
            }
        }
    };
    // Pattern: mode, count, contiguous pointer, and stride (trace level for high frequency)
    ($func:ident => $api:ident(mode: PrimitiveType, count: FxU32, pointers: *const Vertex, stride: FxU32), trace) => {
        #[no_mangle]
        pub extern "system" fn $api(
            mode: PrimitiveType,
            count: FxU32,
            pointers: *const Vertex,
            stride: FxU32,
        ) {
            let mut gr = GR.lock().unwrap();
            if let Some(gr) = (*gr).as_mut() {
                trace!(
                    concat!(stringify!($api), "(mode={:?}, count={:?}, stride={:?})"),
                    mode,
                    count,
                    stride
                );
                gr.$func(mode, count, pointers, stride);
            } else {
                debug!(concat!(stringify!($api), "() called before grGlideInit!"));
            }
        }
    };
}

// Macro for wrapping standalone API functions from glidevk::api
macro_rules! define_standalone {
    // No arguments, no return
    ($impl_fn:path => $api:ident()) => {
        #[no_mangle]
        pub extern "system" fn $api() {
            debug!(concat!(stringify!($api), "()"));
            $impl_fn()
        }
    };
    // With arguments, no return
    ($impl_fn:path => $api:ident($($arg:ident: $arg_type:ty),+)) => {
        #[no_mangle]
        pub extern "system" fn $api($($arg: $arg_type),+) {
            debug!(concat!(stringify!($api), "(", $(stringify!($arg), "={:?}, "),+, ")"), $($arg),+);
            $impl_fn($($arg),+)
        }
    };
    // No arguments, with return
    ($impl_fn:path => $api:ident() -> $ret:ty) => {
        #[no_mangle]
        pub extern "system" fn $api() -> $ret {
            debug!(concat!(stringify!($api), "()"));
            $impl_fn()
        }
    };
    // With arguments, with return
    ($impl_fn:path => $api:ident($($arg:ident: $arg_type:ty),+) -> $ret:ty) => {
        #[no_mangle]
        pub extern "system" fn $api($($arg: $arg_type),+) -> $ret {
            debug!(concat!(stringify!($api), "(", $(stringify!($arg), "={:?}, "),+, ")"), $($arg),+);
            $impl_fn($($arg),+)
        }
    };
}

// glide management functions
lazy_static! {
    static ref GR: Arc<Mutex<Option<Gr>>> = Arc::new(Mutex::new(None));
}
// static mut GR: Lazy<Arc<Mutex<Option<Gr>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

// FX_ENTRY void FX_CALL grGlideInit( void );
#[no_mangle]
pub extern "system" fn grGlideInit() {
    let _ = env_logger::try_init();
    debug!("grGlideInit()");
    let mut gr = GR.lock().unwrap();
    *gr = Some(Gr::init(None));
}

// FX_ENTRY void FX_CALL grGlideShutdown( void );
#[no_mangle]
pub extern "system" fn grGlideShutdown() {
    debug!("grGlideShutdown()");
    let mut gr = GR.lock().unwrap();
    *gr = None;
}

/// Initialize Glide with a window (for windowed rendering)
/// This is a GlideVK extension - not part of original Glide API
pub fn grGlideInitWithWindow(window: winit::window::Window) {
    debug!("grGlideInitWithWindow()");
    let mut gr = GR.lock().unwrap();
    *gr = Some(Gr::init(Some(window)));
}

/// Get a reference to the window (for requesting redraws)
/// This is a GlideVK extension - not part of original Glide API
/// Returns None if Glide was initialized without a window
pub fn grGetWindow<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&winit::window::Window) -> R,
{
    let gr = GR.lock().unwrap();
    if let Some(gr) = (*gr).as_ref() {
        gr.window_ref().map(f)
    } else {
        None
    }
}

define_ptr_function!(lfb_read_region_rgb565 => grLfbReadRegion(src_buffer: Buffer, src_x: FxU32, src_y: FxU32, src_width: FxU32, src_height: FxU32, dst_stride: FxU32; dst_data: *mut std::ffi::c_void) -> FxBool);
define_ptr_function!(lfb_lock => grLfbLock(lock_type: LfbLock, buffer: Buffer, write_mode: LfbWriteMode, origin: OriginLocation, pixel_pipeline: FxBool; info: &mut LfbInfo) -> FxBool);
define_function!(lfb_unlock => grLfbUnlock(lock_type: LfbLock, buffer: Buffer) -> FxBool);

define_function!(sst_select => grSstSelect(which_sst: i32) -> Option<()>);
define_function!(sst_win_open => grSstWinOpen(hwnd: FxU32, res: ScreenResolution, refresh: ScreenRefresh, cformat: ColorFormat, origin: OriginLocation, num_buffers: i32, num_aux_buffers: i32) -> GrContext);
define_function!(sst_win_close => grSstWinClose(ctx: GrContext) -> FxBool);
define_function!(select_context => grSelectContext(ctx: GrContext) -> FxBool);
define_function!(clip_window => grClipWindow(minx: FxU32, miny: FxU32, maxx: FxU32, maxy: FxU32));
define_function!(buffer_clear => grBufferClear(color: Color, alpha: Alpha, depth: FxU32) -> Option<()>);
define_function!(buffer_swap => grBufferSwap(interval: i32) -> Option<()>);
define_function!(sst_screen_width => grSstScreenWidth() -> FxU32);
define_function!(sst_screen_height => grSstScreenHeight() -> FxU32);

// State management functions
define_function!(depth_buffer_mode => grDepthBufferMode(mode: DepthBufferMode));
define_function!(depth_buffer_function => grDepthBufferFunction(fnc: CmpFnc));
define_function!(depth_mask => grDepthMask(mask: FxBool));
define_function!(color_mask => grColorMask(rgb: FxBool, alpha: FxBool));
define_function!(alpha_blend_function => grAlphaBlendFunction(rgb_src: AlphaBlendFnc, rgb_dst: AlphaBlendFnc, alpha_src: AlphaBlendFnc, alpha_dst: AlphaBlendFnc));
define_function!(cull_mode => grCullMode(mode: CullMode));
define_function!(render_buffer => grRenderBuffer(buffer: Buffer));
define_function!(sst_origin => grSstOrigin(origin: OriginLocation));
define_function!(constant_color_value => grConstantColorValue(color: Color));
define_function!(fog_mode => grFogMode(mode: FogMode));
define_function!(fog_color_value => grFogColorValue(color: Color));
define_function!(chromakey_mode => grChromakeyMode(mode: ChromakeyMode));
define_function!(chromakey_value => grChromakeyValue(color: Color));
define_function!(alpha_test_function => grAlphaTestFunction(fnc: CmpFnc));
define_function!(alpha_test_reference_value => grAlphaTestReferenceValue(value: Alpha));

// Enable/disable functions
define_function!(enable => grEnable(mode: EnableMode));
define_function!(disable => grDisable(mode: EnableMode));

// Drawing functions
define_draw_function!(draw_triangle => grDrawTriangle(a: &Vertex, b: &Vertex, c: &Vertex));
define_draw_function!(draw_point => grDrawPoint(a: &Vertex));
define_draw_function!(draw_line => grDrawLine(a: &Vertex, b: &Vertex));

// Vertex array drawing functions
define_vertex_array_function!(draw_vertex_array => grDrawVertexArray(mode: PrimitiveType, count: FxU32, pointers: *const *const Vertex));
define_vertex_array_function!(draw_vertex_array_contiguous => grDrawVertexArrayContiguous(mode: PrimitiveType, count: FxU32, pointers: *const Vertex, stride: FxU32), trace);

// Texture functions
define_function!(tex_filter_mode => grTexFilterMode(tmu: ChipID, min_filter: TextureFilterMode, mag_filter: TextureFilterMode));
define_function!(tex_clamp_mode => grTexClampMode(tmu: ChipID, s_clamp: TextureClampMode, t_clamp: TextureClampMode));
define_function!(tex_mipmap_mode => grTexMipMapMode(tmu: ChipID, mode: MipMapMode, lod_blending: FxBool));
define_function!(tex_lod_bias_value => grTexLodBiasValue(tmu: ChipID, bias: f32));
define_function!(tex_min_address => grTexMinAddress(tmu: ChipID) -> FxU32);
define_function!(tex_max_address => grTexMaxAddress(tmu: ChipID) -> FxU32);
define_function!(tex_combine => grTexCombine(tmu: ChipID, rgb_function: TextureCombineFnc, rgb_factor: CombineFactor, alpha_function: TextureCombineFnc, alpha_factor: CombineFactor, rgb_invert: FxBool, alpha_invert: FxBool));
define_function!(tex_download_mipmap_level => grTexDownloadMipMapLevel(tmu: ChipID, start_address: FxU32, this_lod: LOD, large_lod: LOD, aspect_ratio: AspectRatio, format: TextureFormat, even_odd: FxU32, data: *const std::ffi::c_void));
define_function!(tex_download_mipmap_level_partial => grTexDownloadMipMapLevelPartial(tmu: ChipID, start_address: FxU32, this_lod: LOD, large_lod: LOD, aspect_ratio: AspectRatio, format: TextureFormat, even_odd: FxU32, data: *const std::ffi::c_void, start_row: i32, end_row: i32));
define_function!(tex_download_table => grTexDownloadTable(table_type: TextureTableType, data: *const std::ffi::c_void));

// Texture pointer functions
define_ptr_function!(tex_source => grTexSource(tmu: ChipID, start_address: FxU32, even_odd: FxU32; info: &TexInfo));
define_ptr_function!(tex_download_mipmap => grTexDownloadMipMap(tmu: ChipID, start_address: FxU32, even_odd: FxU32; info: &TexInfo));
define_ptr_function!(tex_texture_mem_required => grTexTextureMemRequired(even_odd: FxU32; info: &TexInfo) -> FxU32);

// Standalone API functions - implementations in glidevk::api
define_standalone!(glidevk::api::color_combine => grColorCombine(function: CombineFunction, factor: CombineFactor, local: CombineLocal, other: CombineOther, invert: FxBool));
define_function!(vertex_layout => grVertexLayout(param: FxU32, offset: FxU32, mode: FxU32));
define_function!(reset => grReset(what: FxU32) -> FxBool);
define_standalone!(glidevk::api::alpha_combine => grAlphaCombine(function: CombineFunction, factor: CombineFactor, local: CombineLocal, other: CombineOther, invert: FxBool));
define_standalone!(glidevk::api::coordinate_space => grCoordinateSpace(mode: CoordinateSpaceMode));
define_function!(dither_mode => grDitherMode(mode: DitherMode));
define_standalone!(glidevk::api::finish => grFinish());
define_standalone!(glidevk::api::get_string => grGetString(pname: EnvironmentInfo) -> *const std::ffi::c_char);
// Explicit grGet implementation to log return values
#[no_mangle]
pub extern "system" fn grGet(pname: GrGetParam, plength: FxU32, params: *mut FxI32) -> FxU32 {
    let result = glidevk::api::get(pname, plength, params);
    let value = if !params.is_null() && result >= 4 {
        unsafe { *params }
    } else {
        0
    };
    debug!(
        "grGet(pname={:?}, plength={}) -> result={}, value={}",
        pname, plength, result, value
    );
    result
}
define_standalone!(glidevk::api::get_proc_address => grGetProcAddress(proc_name: *const std::ffi::c_char) -> GrProc);
define_standalone!(glidevk::api::load_gamma_table => grLoadGammaTable(num_entries: FxU32, red: *const FxU32, green: *const FxU32, blue: *const FxU32));
define_standalone!(glidevk::api::gamma_correction_rgb => guGammaCorrectionRGB(gamma_red: f32, gamma_green: f32, gamma_blue: f32));
