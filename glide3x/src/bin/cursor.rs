//! Windowed mouse-coordinate regression: run with a scaled, resizable glidevk.toml.
#[cfg(target_os = "windows")]
fn main() {
    use glide3x::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
    use winapi::shared::windef::{HWND, POINT, RECT};
    use winapi::um::libloaderapi::GetModuleHandleW;
    use winapi::um::winuser::*;

    const WIDTH: u32 = 640;
    const HEIGHT: u32 = 480;
    static POSITION: AtomicU32 = AtomicU32::new(0);
    static WARPS: AtomicU32 = AtomicU32::new(0);
    static WHEEL: AtomicU32 = AtomicU32::new(0);

    unsafe extern "system" fn game_window(
        hwnd: HWND,
        message: UINT,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if matches!(message, WM_MOUSEWHEEL | WM_MOUSEHWHEEL) {
            WHEEL.store(lparam as u32, Ordering::Relaxed);
            return 0;
        }
        if matches!(message, WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP) {
            let x = lparam as u16 as u32;
            let y = (lparam as u32 >> 16) as u16 as u32;
            POSITION.store(lparam as u32, Ordering::Relaxed);
            // Games may call SetCursorPos here if the window coordinates exceed the render resolution.
            if x >= WIDTH || y >= HEIGHT {
                WARPS.fetch_add(1, Ordering::Relaxed);
            }
            return 0;
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }

    unsafe fn clip_rect() -> (i32, i32, i32, i32) {
        let mut rect: RECT = std::mem::zeroed();
        assert_ne!(GetClipCursor(&mut rect), 0);
        (rect.left, rect.top, rect.right, rect.bottom)
    }

    env_logger::init();
    let config = glidevk::config::Config::load();
    assert!(
        config.window.windowed,
        "This probe requires windowed = true"
    );
    unsafe {
        let class: Vec<u16> = "GlideVkCursorProbe\0".encode_utf16().collect();
        let instance = GetModuleHandleW(std::ptr::null());
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(game_window),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        assert_ne!(RegisterClassW(&window_class), 0);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100,
            100,
            800,
            600,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null_mut(),
        );
        assert!(!hwnd.is_null());
        assert_ne!(ClipCursor(std::ptr::null()), 0);
        let desktop_clip = clip_rect();
        SetForegroundWindow(hwnd);
        assert_eq!(GetForegroundWindow(), hwnd);
        grGlideInit();
        grSstSelect(0);
        let context = grSstWinOpen(
            hwnd as usize as u32,
            ScreenResolution::R640x480,
            ScreenRefresh::R60Hz,
            ColorFormat::Abgr,
            OriginLocation::UpperLeft,
            2,
            1,
        );
        assert_ne!(context, 0);

        for (window_width, window_height) in [(1280, 960), (1280, 1024), (1500, 900)] {
            let (width, height) = glidevk::utility::platforms::configure_native_window(
                hwnd as usize as u32,
                window_width,
                window_height,
                true,
                false,
            )
            .expect("Resize probe window");
            SendMessageW(hwnd, WM_SETFOCUS, 0, 0);
            let mut origin = POINT { x: 0, y: 0 };
            assert_ne!(ClientToScreen(hwnd, &mut origin), 0);
            let expected_clip = if config.window.clip_cursor {
                (
                    origin.x,
                    origin.y,
                    origin.x + width as i32,
                    origin.y + height as i32,
                )
            } else {
                desktop_clip
            };
            assert_eq!(
                clip_rect(),
                expected_clip,
                "Clip covers the entire client area"
            );
            SendMessageW(hwnd, WM_KILLFOCUS, 0, 0);
            assert_eq!(clip_rect(), desktop_clip, "Focus loss releases confinement");
            SendMessageW(hwnd, WM_SETFOCUS, 0, 0);
            assert_eq!(
                clip_rect(),
                expected_clip,
                "Focus gain restores confinement"
            );
            let (vw, vh) = if config.window.keep_aspect_ratio {
                let scale = (width as f64 / WIDTH as f64).min(height as f64 / HEIGHT as f64);
                (
                    (WIDTH as f64 * scale) as u32,
                    (HEIGHT as f64 * scale) as u32,
                )
            } else {
                (width, height)
            };
            let (left, top) = ((width - vw) / 2, (height - vh) / 2);
            for (x, y) in [(WIDTH * 3 / 4, HEIGHT / 2), (0, 0), (WIDTH - 1, HEIGHT - 1)] {
                // Pick a window pixel inside the corresponding game pixel.
                let wx = left + ((x * 2 + 1) as u64 * vw as u64 / (WIDTH * 2) as u64) as u32;
                let wy = top + ((y * 2 + 1) as u64 * vh as u64 / (HEIGHT * 2) as u64) as u32;
                for message in [WM_MOUSEMOVE, WM_LBUTTONDOWN, WM_LBUTTONUP] {
                    SendMessageW(hwnd, message, 0, (wx | (wy << 16)) as LPARAM);
                    let actual = POSITION.load(Ordering::Relaxed);
                    assert_eq!(
                        (actual & 0xffff, actual >> 16),
                        (x, y),
                        "window={width}x{height}, message={message:#x}, raw=({wx},{wy})"
                    );
                    assert_eq!(WARPS.load(Ordering::Relaxed), 0, "Game would warp cursor");
                }
            }
            // Window edges / black bars clamp to the game edge; wheel coordinates stay in screen space.
            for (raw, expected) in [
                (0xffff_ffffu32, 0),
                (
                    (width - 1) | ((height - 1) << 16),
                    (WIDTH - 1) | ((HEIGHT - 1) << 16),
                ),
            ] {
                SendMessageW(hwnd, WM_MOUSEMOVE, 0, raw as LPARAM);
                assert_eq!(POSITION.load(Ordering::Relaxed), expected);
            }
            for message in [WM_MOUSEWHEEL, WM_MOUSEHWHEEL] {
                let screen_position = 1200 | (700 << 16);
                SendMessageW(hwnd, message, 120 << 16, screen_position as LPARAM);
                assert_eq!(WHEEL.load(Ordering::Relaxed), screen_position);
            }
            assert_eq!(WARPS.load(Ordering::Relaxed), 0, "Game would warp cursor");
            let mut client: RECT = std::mem::zeroed();
            assert_ne!(GetClientRect(hwnd, &mut client), 0);
            println!(
                "cursor input passed for {}x{} client",
                client.right, client.bottom
            );
        }

        grSstWinClose(context);
        assert_eq!(
            clip_rect(),
            desktop_clip,
            "Context close releases confinement"
        );
        let raw = 1000 | (700 << 16);
        SendMessageW(hwnd, WM_MOUSEMOVE, 0, raw as LPARAM);
        assert_eq!(
            POSITION.load(Ordering::Relaxed),
            raw,
            "Subclass removed on close"
        );
        grGlideShutdown();
        DestroyWindow(hwnd);
        UnregisterClassW(class.as_ptr(), instance);
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    println!("This cursor regression requires Windows or Wine.");
}
