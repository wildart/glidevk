//! Repeatedly open/close Glide contexts and report 32-bit address-space use.
//! Reproduces repeated mode switches: wine reopen.exe -n 8
use glide3x::*;
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

#[cfg(target_os = "windows")]
fn address_space() -> (u64, u64, u64) {
    use winapi::um::memoryapi::VirtualQuery;
    use winapi::um::winnt::{MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_FREE};

    let (mut free, mut largest, mut committed) = (0u64, 0u64, 0u64);
    let mut addr = 0usize;
    loop {
        let mut mbi = unsafe { std::mem::zeroed::<MEMORY_BASIC_INFORMATION>() };
        if unsafe {
            VirtualQuery(
                addr as *const _,
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        } == 0
        {
            break;
        }
        let size = mbi.RegionSize as u64;
        match mbi.State {
            MEM_FREE => {
                free += size;
                largest = largest.max(size);
            }
            MEM_COMMIT => committed += size,
            _ => {}
        }
        addr = match (mbi.BaseAddress as usize).checked_add(mbi.RegionSize) {
            Some(next) if next > addr => next,
            _ => break,
        };
    }
    (free, largest, committed)
}

#[cfg(not(target_os = "windows"))]
fn address_space() -> (u64, u64, u64) {
    (0, 0, 0)
}

fn report(label: &str) -> u64 {
    let (free, largest, committed) = address_space();
    println!(
        "{label}: free={} MB largest_free={} MB committed={} MB",
        free >> 20,
        largest >> 20,
        committed >> 20
    );
    committed
}

fn churn_textures() -> (u64, u64) {
    const TEXTURES: usize = 16;
    const ROUNDS: usize = 32;
    const SIZE: usize = 256;
    let info = TexInfo::empty(
        LOD::L256,
        LOD::L256,
        AspectRatio::A1x1,
        TextureFormat::Rgb565,
    );
    let mut pixels = vec![0u16; SIZE * SIZE];

    for (param, offset) in [
        (0x01, 0),
        (0x20, std::mem::offset_of!(Vertex, r)),
        (0x10, std::mem::offset_of!(Vertex, a)),
        (0x02, std::mem::offset_of!(Vertex, ooz)),
        (0x04, std::mem::offset_of!(Vertex, oow)),
        (0x40, std::mem::offset_of!(Vertex, tmuvtx)),
        (0x50, std::mem::offset_of!(Vertex, tmuvtx) + 8),
    ] {
        grVertexLayout(param, offset as u32, 1);
    }
    grColorCombine(
        CombineFunction::ScaleOther,
        CombineFactor::One,
        CombineLocal::Iterated,
        CombineOther::Texture,
        FxBool::False,
    );
    grAlphaCombine(
        CombineFunction::ScaleOther,
        CombineFactor::One,
        CombineLocal::Iterated,
        CombineOther::Texture,
        FxBool::False,
    );

    let before = report("texture churn before");
    for round in 0..ROUNDS {
        for texture in 0..TEXTURES {
            pixels.fill((((round + texture) as u16) << 5) | 0x001f);
            grTexDownloadMipMapLevel(
                ChipID::Tmu0,
                0x1000 + texture as u32 * (SIZE * SIZE * 2) as u32,
                LOD::L256,
                LOD::L256,
                AspectRatio::A1x1,
                TextureFormat::Rgb565,
                3,
                pixels.as_ptr().cast(),
            );
        }

        grBufferClear(Color(0), Alpha(0), 0xffff);
        for texture in 0..TEXTURES {
            let x = (texture % 4) as f32 * 160.0;
            let y = (texture / 4) as f32 * 120.0;
            let vertex = |x, y, s, t| {
                let mut v = Vertex::new(x, y);
                v.tmuvtx[0].sow = s;
                v.tmuvtx[0].tow = t;
                v
            };
            let a = vertex(x, y, 0.0, 0.0);
            let b = vertex(x + 160.0, y, 256.0, 0.0);
            let c = vertex(x + 160.0, y + 120.0, 256.0, 256.0);
            let d = vertex(x, y + 120.0, 0.0, 256.0);
            grTexSource(
                ChipID::Tmu0,
                0x1000 + texture as u32 * (SIZE * SIZE * 2) as u32,
                3,
                &info,
            );
            grDrawTriangle(&a, &b, &c);
            grDrawTriangle(&a, &c, &d);
        }
        grBufferSwap(1);
    }
    let after = report("texture churn after ");
    println!(
        "texture churn growth: {} MB",
        after.saturating_sub(before) >> 20
    );
    (before, after)
}

fn main() {
    env_logger::init();
    let cycles: usize = std::env::args()
        .skip_while(|a| a != "-n")
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(6);
    let event_loop = EventLoop::new().expect("event loop");
    let window = WindowBuilder::new()
        .with_title("reopen")
        .with_inner_size(winit::dpi::LogicalSize::new(800, 600))
        .build(&event_loop)
        .expect("window");
    #[cfg(target_os = "windows")]
    let hwnd = {
        use winit::raw_window_handle::HasWindowHandle;
        match window.window_handle().expect("handle").as_raw() {
            winit::raw_window_handle::RawWindowHandle::Win32(h) => h.hwnd.get() as u32,
            _ => unreachable!(),
        }
    };
    #[cfg(not(target_os = "windows"))]
    let hwnd = 0u32;
    let _ = &window;

    report("start");
    let mut baseline = None;
    for cycle in 1..=cycles {
        let res = if cycle % 2 == 0 {
            ScreenResolution::R640x480
        } else {
            ScreenResolution::R800x600
        };
        grGlideInit();
        grSstSelect(0);
        let ctx = grSstWinOpen(
            hwnd,
            res,
            ScreenRefresh::R60Hz,
            ColorFormat::Abgr,
            OriginLocation::UpperLeft,
            2,
            1,
        );
        assert_eq!(ctx, 1, "grSstWinOpen failed");
        report(&format!("cycle {cycle} open  "));
        if cycle == 1 {
            let (before, after) = churn_textures();
            let growth = after.saturating_sub(before);
            assert!(
                growth <= 16 << 20,
                "texture churn grew committed memory by {} MB",
                growth >> 20
            );
        }
        grBufferClear(Color(0), Alpha(0), 0xffff);
        grBufferSwap(1);
        grSstWinClose(ctx);
        grGlideShutdown();
        let committed = report(&format!("cycle {cycle} closed"));
        let baseline = *baseline.get_or_insert(committed);
        assert!(
            committed <= baseline + (8 << 20),
            "address space grows {} MB per cycle; Wine host-memory leak is back",
            committed.saturating_sub(baseline) >> 20
        );
    }
    println!("done");
}
