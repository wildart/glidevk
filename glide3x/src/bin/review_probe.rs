use glidevk::{glidesys::*, Gr};
use winit::{event_loop::EventLoop, window::WindowBuilder};

fn read_pixel(gr: &mut Gr, x: u32, y: u32) -> u16 {
    let mut value = 0u16;
    assert_eq!(
        gr.lfb_read_region_rgb565(
            Buffer::Front,
            x,
            y,
            1,
            1,
            2,
            &mut value as *mut u16 as *mut _,
        ),
        FxBool::True,
    );
    value
}

fn write(gr: &mut Gr, x: u32, y: u32, width: u32, height: u32, color: u16) {
    let pixels = vec![color; (width * height) as usize];
    assert_eq!(
        gr.lfb_write_region(
            Buffer::Back,
            x,
            y,
            LfbWriteMode::Rgb565,
            width,
            height,
            FxBool::False,
            (width * 2) as i32,
            pixels.as_ptr() as *const _,
        ),
        FxBool::True,
    );
}

fn triangle(gr: &mut Gr) {
    gr.color_combine(
        CombineFunction::Local,
        CombineFactor::Zero,
        CombineLocal::Constant,
        CombineOther::Constant,
        FxBool::False,
    );
    gr.constant_color_value(Color(0xff00ff00));
    gr.draw_triangle(
        &Vertex::new(100.0, 100.0),
        &Vertex::new(500.0, 100.0),
        &Vertex::new(300.0, 400.0),
    );
}

fn main() {
    let mode = std::env::args()
        .nth(1)
        .expect("partial | mixed | scaled | frames");
    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new()
        .with_visible(false)
        .with_inner_size(winit::dpi::PhysicalSize::new(640, 480))
        .build(&event_loop)
        .unwrap();
    let hwnd = {
        #[cfg(target_os = "windows")]
        {
            use winit::raw_window_handle::HasWindowHandle;
            match window
                .window_handle()
                .expect("Native window handle")
                .as_raw()
            {
                winit::raw_window_handle::RawWindowHandle::Win32(h) => h.hwnd.get() as u32,
                _ => unreachable!(),
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            0
        }
    };
    let mut gr = Gr::init(Some(window));
    assert_eq!(
        gr.sst_win_open(
            hwnd,
            ScreenResolution::R640x480,
            ScreenRefresh::R75Hz,
            ColorFormat::Argb,
            OriginLocation::UpperLeft,
            2,
            1,
        ),
        1
    );
    match mode.as_str() {
        "partial" => {
            write(&mut gr, 0, 0, 640, 480, 0x001f);
            gr.buffer_swap(1).unwrap();
            println!("BASELINE outside={:#06x}", read_pixel(&mut gr, 20, 20));
            write(&mut gr, 10, 10, 1, 1, 0xf800);
            println!(
                "BEFORE_SWAP front_pixel={:#06x} expected=0x001f",
                read_pixel(&mut gr, 10, 10)
            );
            gr.buffer_swap(1).unwrap();
            println!(
                "PARTIAL outside={:#06x} expected=0x001f",
                read_pixel(&mut gr, 20, 20)
            );
        }
        "mixed" => {
            gr.buffer_clear(Color(0xff0000ff), Alpha(255), 65535)
                .unwrap();
            write(&mut gr, 10, 10, 1, 1, 0xf800);
            triangle(&mut gr);
            gr.buffer_swap(1).unwrap();
            println!(
                "MIXED triangle={:#06x} expected=0x07e0",
                read_pixel(&mut gr, 300, 200)
            );
        }
        "scaled" => {
            write(&mut gr, 0, 0, 640, 480, 0xf800);
            gr.buffer_swap(1).unwrap();
            println!(
                "SCALED edge={:#06x} expected=0xf800",
                read_pixel(&mut gr, 0, 240)
            );
            println!(
                "SCALED center={:#06x} expected=0xf800",
                read_pixel(&mut gr, 320, 240)
            );
        }
        "frames" => {
            for _ in 0..10 {
                gr.buffer_clear(Color(0), Alpha(255), 65535).unwrap();
                triangle(&mut gr);
                gr.buffer_swap(1).unwrap();
            }
            println!("FRAMES completed=10");
        }
        "state" => check_state(&mut gr),
        "textures" => check_textures(&mut gr),
        "capacity" => check_capacity(&mut gr),
        "updates" => check_updates(&mut gr),
        "samplers" => check_samplers(&mut gr),
        "locks-ui" => check_locks_and_ui(&mut gr),
        _ => panic!("unknown probe"),
    }
    drop(gr);
    assert_eq!(
        glidevk::utility::debug::validation_errors(),
        0,
        "Vulkan validation errors"
    );
}

fn local_color(gr: &mut Gr, color: u32) {
    gr.color_combine(
        CombineFunction::Local,
        CombineFactor::Zero,
        CombineLocal::Constant,
        CombineOther::Constant,
        FxBool::False,
    );
    gr.alpha_combine(
        CombineFunction::Local,
        CombineFactor::Zero,
        CombineLocal::Constant,
        CombineOther::Constant,
        FxBool::False,
    );
    gr.constant_color_value(Color(color));
}
fn quad(gr: &mut Gr, x: f32, y: f32, w: f32, h: f32, z: f32) {
    let make = |x, y, s, t| {
        let mut v = Vertex::new(x, y);
        v.ooz = z;
        v.tmuvtx[0].sow = s;
        v.tmuvtx[0].tow = t;
        v
    };
    let a = make(x, y, 0.0, 0.0);
    let b = make(x + w, y, 256.0, 0.0);
    let c = make(x + w, y + h, 256.0, 256.0);
    let d = make(x, y + h, 0.0, 256.0);
    gr.draw_triangle(&a, &b, &c);
    gr.draw_triangle(&a, &c, &d);
}
fn back(gr: &mut Gr, x: u32, y: u32) -> u16 {
    let mut pixel = 0;
    assert_eq!(
        gr.lfb_read_region_rgb565(Buffer::Back, x, y, 1, 1, 2, (&mut pixel as *mut u16).cast()),
        FxBool::True
    );
    pixel
}
fn check_state(gr: &mut Gr) {
    gr.buffer_clear(Color(0xff0000ff), Alpha(255), 65535)
        .unwrap();
    gr.clip_window(10, 10, 30, 30);
    gr.buffer_clear(Color(0xffff0000), Alpha(255), 65535)
        .unwrap();
    assert_eq!(back(gr, 0, 0), 0x001f, "clipped clear preserves outside");
    assert_eq!(back(gr, 20, 20), 0xf800, "clipped clear");
    local_color(gr, 0xff00ff00);
    quad(gr, 0.0, 0.0, 100.0, 100.0, 0.0);
    gr.clip_window(100, 100, 200, 200);
    local_color(gr, 0xffff0000);
    quad(gr, 0.0, 0.0, 300.0, 300.0, 0.0);
    assert_eq!(back(gr, 20, 20), 0x07e0, "draw-time clip");
    assert_eq!(back(gr, 150, 150), 0xf800);
    gr.clip_window(0, 0, 640, 480);
    gr.buffer_clear(Color(0), Alpha(255), 65535).unwrap();
    gr.depth_buffer_mode(DepthBufferMode::ZBuffer);
    gr.depth_buffer_function(CmpFnc::Less);
    gr.depth_mask(FxBool::True);
    gr.buffer_clear(Color(0), Alpha(255), 65535).unwrap();
    local_color(gr, 0xffff0000);
    quad(gr, 0.0, 0.0, 100.0, 100.0, 10000.0);
    local_color(gr, 0xff00ff00);
    quad(gr, 0.0, 0.0, 100.0, 100.0, 50000.0);
    assert_eq!(back(gr, 10, 10), 0xf800, "Z depth comparison");
    gr.depth_buffer_mode(DepthBufferMode::Disable);
    quad(gr, 0.0, 0.0, 100.0, 100.0, 50000.0);
    assert_eq!(back(gr, 10, 10), 0x07e0, "depth disabled");
    gr.buffer_clear(Color(0), Alpha(255), 65535).unwrap();
    gr.depth_buffer_mode(DepthBufferMode::ZBuffer);
    quad(gr, 0.0, 0.0, 100.0, 100.0, 50000.0);
    assert_eq!(
        back(gr, 10, 10),
        0x07e0,
        "depth clear honors write mask while depth testing is disabled"
    );
    gr.depth_buffer_mode(DepthBufferMode::Disable);
    gr.alpha_test_function(CmpFnc::Never);
    local_color(gr, 0xffff0000);
    quad(gr, 0.0, 0.0, 100.0, 100.0, 0.0);
    gr.alpha_test_function(CmpFnc::Always);
    assert_eq!(back(gr, 10, 10), 0x07e0, "alpha NEVER");
    gr.color_mask(FxBool::False, FxBool::False);
    quad(gr, 0.0, 0.0, 100.0, 100.0, 0.0);
    gr.color_mask(FxBool::True, FxBool::True);
    assert_eq!(back(gr, 10, 10), 0x07e0, "color mask");
    gr.color_combine(
        CombineFunction::Local,
        CombineFactor::Zero,
        CombineLocal::Constant,
        CombineOther::Constant,
        FxBool::True,
    );
    quad(gr, 0.0, 0.0, 100.0, 100.0, 0.0);
    assert_eq!(back(gr, 10, 10), 0x07ff, "invert red to cyan");
    gr.buffer_clear(Color(0xff0000ff), Alpha(255), 65535)
        .unwrap();
    local_color(gr, 0x80ff0000);
    gr.alpha_blend_function(
        AlphaBlendFnc::SrcAlpha,
        AlphaBlendFnc::One,
        AlphaBlendFnc::One,
        AlphaBlendFnc::Zero,
    );
    quad(gr, 0.0, 0.0, 100.0, 100.0, 0.0);
    let pixel = back(gr, 10, 10);
    assert_eq!(pixel & 31, 31, "additive blend retains destination");
    assert!((15..=16).contains(&(pixel >> 11)), "additive source alpha");
    gr.alpha_blend_function(
        AlphaBlendFnc::One,
        AlphaBlendFnc::Zero,
        AlphaBlendFnc::One,
        AlphaBlendFnc::Zero,
    );
    gr.fog_table([255u8; 64].as_ptr());
    gr.fog_color_value(Color(0xff00ff00));
    gr.fog_mode(FogMode::WithTable);
    local_color(gr, 0xffff0000);
    quad(gr, 0.0, 0.0, 100.0, 100.0, 0.0);
    gr.fog_mode(FogMode::Disable);
    quad(gr, 100.0, 0.0, 100.0, 100.0, 0.0);
    assert_eq!(back(gr, 10, 10), 0x07e0, "draw-time fog table");
    assert_eq!(back(gr, 110, 10), 0xf800, "fog disabled for later draw");
    gr.buffer_swap(1).unwrap();
    println!("STATE passed");
}
fn bind(gr: &mut Gr, address: u32, lod: LOD) {
    let info = TexInfo::empty(lod, lod, AspectRatio::A1x1, TextureFormat::Rgb565);
    gr.tex_source(ChipID::Tmu0, address, 3, &info);
}
fn download(gr: &mut Gr, address: u32, color: u16) {
    gr.tex_download_mipmap_level(
        ChipID::Tmu0,
        address,
        LOD::L1,
        LOD::L1,
        AspectRatio::A1x1,
        TextureFormat::Rgb565,
        3,
        (&color as *const u16).cast(),
    );
}
fn texture_color(gr: &mut Gr) {
    gr.color_combine(
        CombineFunction::ScaleOther,
        CombineFactor::One,
        CombineLocal::Iterated,
        CombineOther::Texture,
        FxBool::False,
    );
    gr.alpha_combine(
        CombineFunction::Local,
        CombineFactor::One,
        CombineLocal::Iterated,
        CombineOther::Iterated,
        FxBool::False,
    );
}
fn check_textures(gr: &mut Gr) {
    gr.buffer_clear(Color(0), Alpha(255), 65535).unwrap();
    texture_color(gr);
    download(gr, 0, 0xf800);
    bind(gr, 0, LOD::L1);
    quad(gr, 0.0, 0.0, 100.0, 100.0, 0.0);
    download(gr, 0, 0x07e0);
    quad(gr, 100.0, 0.0, 100.0, 100.0, 0.0);
    download(gr, 8, 0x001f);
    quad(gr, 200.0, 0.0, 100.0, 100.0, 0.0);
    assert_eq!(back(gr, 10, 10), 0xf800, "address zero and old content");
    assert_eq!(back(gr, 110, 10), 0x07e0, "new texture content");
    assert_eq!(back(gr, 210, 10), 0x07e0, "download does not rebind");
    gr.tex_combine(
        ChipID::Tmu0,
        CombineFunction::Zero,
        CombineFactor::One,
        CombineFunction::Local,
        CombineFactor::One,
        FxBool::False,
        FxBool::False,
    );
    quad(gr, 300.0, 0.0, 100.0, 100.0, 0.0);
    assert_eq!(back(gr, 310, 10), 0, "TMU ZERO");
    gr.tex_combine(
        ChipID::Tmu0,
        CombineFunction::Local,
        CombineFactor::One,
        CombineFunction::Local,
        CombineFactor::One,
        FxBool::False,
        FxBool::False,
    );
    download(gr, 0, 0xf81f);
    gr.chromakey_value(Color(0xffff00ff));
    gr.chromakey_mode(ChromakeyMode::Enable);
    quad(gr, 400.0, 0.0, 100.0, 100.0, 0.0);
    assert_eq!(back(gr, 410, 10), 0, "configured chromakey");
    gr.chromakey_mode(ChromakeyMode::Disable);
    gr.buffer_swap(1).unwrap();
    gr.buffer_swap(1).unwrap();
    assert_eq!(
        read_pixel(gr, 10, 10),
        0xf800,
        "swap does not replay resolved texture IDs"
    );
    println!("TEXTURES passed");
}
fn check_capacity(gr: &mut Gr) {
    gr.buffer_clear(Color(0), Alpha(255), 65535).unwrap();
    texture_color(gr);
    for i in 0..2100 {
        download(gr, i * 8, if i % 2 == 0 { 0xf800 } else { 0x07e0 });
    }
    for i in 0..2100 {
        bind(gr, i * 8, LOD::L1);
        quad(gr, (i % 600) as f32, (i / 600) as f32, 1.0, 1.0, 0.0);
    }
    gr.buffer_swap(1).unwrap();
    for i in [0, 1000, 2046, 2099] {
        assert_eq!(
            read_pixel(gr, i % 600, i / 600),
            if i % 2 == 0 { 0xf800 } else { 0x07e0 }
        );
    }
    for i in 2100..4200 {
        download(gr, i * 8, 0x001f);
    }
    for i in 2100..4200 {
        bind(gr, i * 8, LOD::L1);
        quad(gr, (i % 600) as f32, 20.0, 1.0, 1.0, 0.0);
    }
    gr.buffer_swap(1).unwrap();
    assert_eq!(read_pixel(gr, 0, 20), 0x001f);
    println!("CAPACITY passed");
}

fn check_updates(gr: &mut Gr) {
    gr.buffer_clear(Color(0), Alpha(255), 65535).unwrap();
    texture_color(gr);
    let level = |gr: &mut Gr, lod, pixels: &[u16]| {
        gr.tex_download_mipmap_level(
            ChipID::Tmu0,
            0,
            lod,
            LOD::L2,
            AspectRatio::A1x1,
            TextureFormat::Rgb565,
            3,
            pixels.as_ptr().cast(),
        )
    };
    level(gr, LOD::L2, &[0xf800; 4]);
    level(gr, LOD::L1, &[0x07e0]);
    let info = TexInfo::empty(LOD::L1, LOD::L2, AspectRatio::A1x1, TextureFormat::Rgb565);
    gr.tex_source(ChipID::Tmu0, 0, 3, &info);
    quad(gr, 0.0, 0.0, 100.0, 100.0, 0.0);
    gr.tex_mipmap_mode(ChipID::Tmu0, MipMapMode::Nearest, FxBool::False);
    quad(gr, 100.0, 0.0, 1.0, 1.0, 0.0);
    assert_eq!(
        back(gr, 20, 20),
        0xf800,
        "smaller mip does not replace the base"
    );
    assert_eq!(
        back(gr, 100, 0),
        0x07e0,
        "minification selects uploaded smaller mip"
    );
    gr.tex_mipmap_mode(ChipID::Tmu0, MipMapMode::Disable, FxBool::False);
    gr.tex_download_mipmap_level_partial(
        ChipID::Tmu0,
        0,
        LOD::L2,
        LOD::L2,
        AspectRatio::A1x1,
        TextureFormat::Rgb565,
        3,
        [0x001fu16; 2].as_ptr().cast(),
        1,
        1,
    );
    quad(gr, 100.0, 0.0, 100.0, 100.0, 0.0);
    assert_eq!(
        back(gr, 110, 25),
        0xf800,
        "partial mip preserves other rows"
    );
    assert_eq!(
        back(gr, 110, 75),
        0x001f,
        "inclusive single-row update reaches GPU"
    );
    gr.tex_download_mipmap_level_partial(
        ChipID::Tmu0,
        0,
        LOD::L2,
        LOD::L2,
        AspectRatio::A1x1,
        TextureFormat::Rgb565,
        3,
        [0x07e0u16; 4].as_ptr().cast(),
        0,
        1,
    );
    quad(gr, 200.0, 0.0, 100.0, 100.0, 0.0);
    assert_eq!(back(gr, 210, 25), 0x07e0);
    assert_eq!(back(gr, 210, 75), 0x07e0);
    assert_eq!(
        back(gr, 20, 20),
        0xf800,
        "earlier draw preserves old content"
    );
    let mut palette = [0u32; 256];
    palette[7] = 0x00ff0000;
    gr.tex_download_table(TextureTableType::Palette, palette.as_ptr().cast());
    gr.tex_download_mipmap_level(
        ChipID::Tmu0,
        64,
        LOD::L1,
        LOD::L1,
        AspectRatio::A1x1,
        TextureFormat::P8,
        3,
        [7u8].as_ptr().cast(),
    );
    gr.tex_source(
        ChipID::Tmu0,
        64,
        3,
        &TexInfo::empty(LOD::L1, LOD::L1, AspectRatio::A1x1, TextureFormat::P8),
    );
    quad(gr, 300.0, 0.0, 100.0, 100.0, 0.0);
    palette[7] = 0x0000ff00;
    gr.tex_download_table(TextureTableType::Palette, palette.as_ptr().cast());
    quad(gr, 400.0, 0.0, 100.0, 100.0, 0.0);
    assert_eq!(back(gr, 310, 10), 0xf800, "draw captures old palette");
    assert_eq!(back(gr, 410, 10), 0x07e0, "draw sees new palette");
    // Both TMUs feed the combine chain.
    gr.tex_download_mipmap_level(
        ChipID::Tmu1,
        0,
        LOD::L1,
        LOD::L1,
        AspectRatio::A1x1,
        TextureFormat::Rgb565,
        3,
        [0x001fu16].as_ptr().cast(),
    );
    gr.tex_source(
        ChipID::Tmu1,
        0,
        3,
        &TexInfo::empty(LOD::L1, LOD::L1, AspectRatio::A1x1, TextureFormat::Rgb565),
    );
    gr.tex_combine(
        ChipID::Tmu1,
        CombineFunction::Local,
        CombineFactor::One,
        CombineFunction::Local,
        CombineFactor::One,
        FxBool::False,
        FxBool::False,
    );
    gr.tex_combine(
        ChipID::Tmu0,
        CombineFunction::ScaleOther,
        CombineFactor::One,
        CombineFunction::ScaleOther,
        CombineFactor::One,
        FxBool::False,
        FxBool::False,
    );
    quad(gr, 500.0, 0.0, 100.0, 100.0, 0.0);
    assert_eq!(back(gr, 510, 10), 0x001f, "TMU1 feeds TMU0 other");
    gr.tex_combine(
        ChipID::Tmu0,
        CombineFunction::Local,
        CombineFactor::One,
        CombineFunction::Local,
        CombineFactor::One,
        FxBool::False,
        FxBool::False,
    );
    gr.tex_download_mipmap_level(
        ChipID::Tmu0,
        128,
        LOD::L4,
        LOD::L4,
        AspectRatio::A1x1,
        TextureFormat::Rgb565,
        3,
        [0xf800u16; 16].as_ptr().cast(),
    );
    gr.tex_download_mipmap_level(
        ChipID::Tmu0,
        128,
        LOD::L1,
        LOD::L4,
        AspectRatio::A1x1,
        TextureFormat::Rgb565,
        3,
        [0x07e0u16].as_ptr().cast(),
    );
    gr.tex_source(
        ChipID::Tmu0,
        128,
        3,
        &TexInfo::empty(LOD::L1, LOD::L4, AspectRatio::A1x1, TextureFormat::Rgb565),
    );
    gr.tex_mipmap_mode(ChipID::Tmu0, MipMapMode::Nearest, FxBool::False);
    quad(gr, 0.0, 100.0, 2.0, 2.0, 0.0);
    assert_eq!(
        back(gr, 0, 100),
        0xf800,
        "missing mip clamps to available base, not black"
    );
    gr.tex_download_mipmap_level(
        ChipID::Tmu0,
        128,
        LOD::L2,
        LOD::L4,
        AspectRatio::A1x1,
        TextureFormat::Rgb565,
        3,
        [0x001fu16; 4].as_ptr().cast(),
    );
    quad(gr, 2.0, 100.0, 2.0, 2.0, 0.0);
    assert_eq!(
        back(gr, 2, 100),
        0x001f,
        "newly completed chain reaches GPU"
    );
    println!("UPDATES passed");
}

fn sample_quad(gr: &mut Gr, x: f32, s: f32) {
    let make = |x, y| {
        let mut v = Vertex::new(x, y);
        v.tmuvtx[0].sow = s;
        v.tmuvtx[0].tow = 64.0;
        v
    };
    let a = make(x, 0.0);
    let b = make(x + 50.0, 0.0);
    let c = make(x + 50.0, 50.0);
    let d = make(x, 50.0);
    gr.draw_triangle(&a, &b, &c);
    gr.draw_triangle(&a, &c, &d);
}
fn check_samplers(gr: &mut Gr) {
    gr.buffer_clear(Color(0), Alpha(255), 65535).unwrap();
    texture_color(gr);
    gr.tex_download_mipmap_level(
        ChipID::Tmu0,
        0,
        LOD::L2,
        LOD::L2,
        AspectRatio::A2x1,
        TextureFormat::Rgb565,
        3,
        [0xf800u16, 0x001f].as_ptr().cast(),
    );
    gr.tex_source(
        ChipID::Tmu0,
        0,
        3,
        &TexInfo::empty(LOD::L2, LOD::L2, AspectRatio::A2x1, TextureFormat::Rgb565),
    );
    gr.tex_filter_mode(
        ChipID::Tmu0,
        TextureFilterMode::PointSampled,
        TextureFilterMode::PointSampled,
    );
    gr.tex_clamp_mode(
        ChipID::Tmu0,
        TextureClampMode::Clamp,
        TextureClampMode::Clamp,
    );
    sample_quad(gr, 0.0, 320.0);
    gr.tex_clamp_mode(
        ChipID::Tmu0,
        TextureClampMode::Wrap,
        TextureClampMode::Clamp,
    );
    sample_quad(gr, 50.0, 320.0);
    gr.tex_filter_mode(
        ChipID::Tmu0,
        TextureFilterMode::Bilinear,
        TextureFilterMode::Bilinear,
    );
    sample_quad(gr, 100.0, 128.0);
    assert_eq!(back(gr, 10, 10), 0x001f, "draw-time clamp");
    assert_eq!(back(gr, 60, 10), 0xf800, "draw-time wrap");
    let mixed = back(gr, 110, 10);
    assert!(
        (15..=16).contains(&(mixed >> 11)) && (15..=16).contains(&(mixed & 31)),
        "bilinear midpoint {mixed:#06x}"
    );
    println!("SAMPLERS passed");
}

fn check_locks_and_ui(gr: &mut Gr) {
    gr.buffer_clear(Color(0xff0000ff), Alpha(255), 65535)
        .unwrap();
    gr.buffer_swap(1).unwrap();
    for (format, expected) in [
        (LfbWriteMode::Rgb565, vec![0x1f, 0]),
        (LfbWriteMode::Rgb555, vec![0x1f, 0]),
        (LfbWriteMode::Argb1555, vec![0x1f, 0x80]),
        (LfbWriteMode::Rgb888, vec![255, 0, 0]),
        (LfbWriteMode::Argb8888, vec![255, 0, 0, 255]),
    ] {
        let mut info = LfbInfo::default();
        assert_eq!(
            gr.lfb_lock(
                LfbLock::WriteOnly | LfbLock::NoIdle,
                Buffer::Back,
                format,
                OriginLocation::LowerLeft,
                FxBool(255),
                &mut info
            ),
            FxBool::True
        );
        unsafe {
            assert_eq!(
                std::slice::from_raw_parts(info.lfb_ptr.cast::<u8>(), expected.len()),
                expected
            );
        }
        assert_eq!(
            gr.lfb_unlock(LfbLock::WriteOnly | LfbLock::NoIdle, Buffer::Back),
            FxBool::True
        );
        assert_eq!(
            back(gr, 500, 400),
            0x001f,
            "unmodified write lock preserves pixels"
        );
    }
    // Alpha-combine ZERO compatibility must retain vertex alpha.
    local_color(gr, 0xffff0000);
    gr.alpha_combine(
        CombineFunction::Zero,
        CombineFactor::Zero,
        CombineLocal::Iterated,
        CombineOther::Iterated,
        FxBool::False,
    );
    gr.alpha_blend_function(
        AlphaBlendFnc::SrcAlpha,
        AlphaBlendFnc::OneMinusSrcAlpha,
        AlphaBlendFnc::One,
        AlphaBlendFnc::Zero,
    );
    let mut a = Vertex::new(0.0, 0.0);
    let mut b = Vertex::new(100.0, 0.0);
    let mut c = Vertex::new(0.0, 100.0);
    a.a = 128.0;
    b.a = 128.0;
    c.a = 128.0;
    gr.draw_triangle(&a, &b, &c);
    let pixel = back(gr, 10, 10);
    assert!(
        (15..=16).contains(&(pixel >> 11)) && (15..=16).contains(&(pixel & 31)),
        "UI alpha {pixel:#06x}"
    );
    println!("LOCKS_UI passed");
}
