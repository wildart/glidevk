//! Test utilities for Glide 3.x test programs
//!
//! Provides common functionality for running tests using the Glide API.

use crate::{
    grGetWindow, grGlideInitWithWindow, grGlideShutdown, grLfbReadRegion, grSstScreenHeight,
    grSstScreenWidth, grSstSelect, grSstWinOpen, Buffer, ColorFormat, FxBool, OriginLocation,
    ScreenRefresh, ScreenResolution,
};
use std::env;
use std::fs::File;
use std::io::Write;
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowBuilder;

/// Re-export keyboard types for test programs
pub use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};

/// Test configuration parsed from command line arguments
#[derive(Clone)]
pub struct TestConfig {
    /// Number of frames to render (-1 for infinite)
    pub frames: i32,
    /// Screen resolution enum for Glide
    pub resolution: ScreenResolution,
    /// Screen width in pixels
    pub width: u32,
    /// Screen height in pixels
    pub height: u32,
    /// Optional filename to dump screen buffer to (enables single frame mode)
    pub dump_file: Option<String>,
    /// Run in headless mode (no visible window)
    pub headless: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            frames: -1,
            resolution: ScreenResolution::R640x480,
            width: 640,
            height: 480,
            dump_file: None,
            headless: false,
        }
    }
}

impl TestConfig {
    /// Screen width as f32 for coordinate calculations
    pub fn width_f(&self) -> f32 {
        self.width as f32
    }

    /// Screen height as f32 for coordinate calculations
    pub fn height_f(&self) -> f32 {
        self.height as f32
    }

    /// Scale normalized X coordinate (0.0-1.0) to screen coordinates
    pub fn scale_x(&self, pos: f32) -> f32 {
        pos * self.width as f32
    }

    /// Scale normalized Y coordinate (0.0-1.0) to screen coordinates
    pub fn scale_y(&self, pos: f32) -> f32 {
        pos * self.height as f32
    }
}

/// Parse resolution string like "640x480" into ScreenResolution and dimensions
pub fn parse_resolution(s: &str) -> Option<(ScreenResolution, u32, u32)> {
    match s {
        "320x200" => Some((ScreenResolution::R320x200, 320, 200)),
        "320x240" => Some((ScreenResolution::R320x240, 320, 240)),
        "400x256" => Some((ScreenResolution::R400x256, 400, 256)),
        "400x300" => Some((ScreenResolution::R400x300, 400, 300)),
        "512x256" => Some((ScreenResolution::R512x256, 512, 256)),
        "512x384" => Some((ScreenResolution::R512x384, 512, 384)),
        "640x200" => Some((ScreenResolution::R640x200, 640, 200)),
        "640x350" => Some((ScreenResolution::R640x350, 640, 350)),
        "640x400" => Some((ScreenResolution::R640x400, 640, 400)),
        "640x480" => Some((ScreenResolution::R640x480, 640, 480)),
        "800x600" => Some((ScreenResolution::R800x600, 800, 600)),
        "856x480" => Some((ScreenResolution::R856x480, 856, 480)),
        "960x720" => Some((ScreenResolution::R960x720, 960, 720)),
        "1024x768" => Some((ScreenResolution::R1024x768, 1024, 768)),
        "1280x1024" => Some((ScreenResolution::R1280x1024, 1280, 1024)),
        "1600x1200" => Some((ScreenResolution::R1600x1200, 1600, 1200)),
        _ => None,
    }
}

/// Get list of available resolutions as a string
pub fn get_resolution_list() -> &'static str {
    "320x200, 320x240, 400x256, 400x300, 512x256, 512x384,\n\
     640x200, 640x350, 640x400, 640x480, 800x600, 856x480,\n\
     960x720, 1024x768, 1280x1024, 1600x1200"
}

/// Print usage information for a test program
pub fn print_usage(name: &str) {
    println!(
        "Usage: {} [-n <frames>] [-r <resolution>] [-d <filename>] [--visible]",
        name
    );
    println!("  -n <frames>     Number of frames to render (-1 for infinite)");
    println!("  -r <resolution> Screen resolution");
    println!("  -d <filename>   Dump screen buffer to file (PPM format, renders 1 frame)");
    println!("  --visible       Keep the window visible while dumping (needed by Wine on macOS)");
    println!("\nAvailable resolutions:\n  {}", get_resolution_list());
}

/// Parse command line arguments and return test configuration
/// Returns None if help was requested or an error occurred
pub fn parse_args() -> Option<TestConfig> {
    let args: Vec<String> = env::args().collect();
    let mut config = TestConfig::default();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-n" => {
                if i + 1 < args.len() {
                    config.frames = args[i + 1].parse().unwrap_or(-1);
                    i += 2;
                } else {
                    print_usage(&args[0]);
                    return None;
                }
            }
            "-r" => {
                if i + 1 < args.len() {
                    if let Some((res, w, h)) = parse_resolution(&args[i + 1]) {
                        config.resolution = res;
                        config.width = w;
                        config.height = h;
                    } else {
                        println!("Unknown resolution: {}", args[i + 1]);
                        print_usage(&args[0]);
                        return None;
                    }
                    i += 2;
                } else {
                    print_usage(&args[0]);
                    return None;
                }
            }
            "-d" => {
                if i + 1 < args.len() {
                    config.dump_file = Some(args[i + 1].clone());
                    config.frames = 1; // Single frame mode for dump
                    config.headless = true; // No visible window for dump mode
                    i += 2;
                } else {
                    print_usage(&args[0]);
                    return None;
                }
            }
            "--visible" => i += 1,
            "-h" | "--help" => {
                print_usage(&args[0]);
                return None;
            }
            _ => {
                println!("Unknown argument: {}", args[i]);
                print_usage(&args[0]);
                return None;
            }
        }
    }

    if args.iter().any(|arg| arg == "--visible") {
        config.headless = false;
    }
    Some(config)
}

/// Save pixel data as a PPM image file
/// Pixel data should be in RGB565 format (2 bytes per pixel)
pub fn screen_dump(filename: &str, width: u32, height: u32, pixels: &[u8]) -> std::io::Result<()> {
    let mut file = File::create(filename)?;

    // Write PPM header (P6 = binary RGB)
    writeln!(file, "P6")?;
    writeln!(file, "{} {}", width, height)?;
    writeln!(file, "255")?;

    // Write pixel data (convert RGB565 to RGB)
    let mut rgb_data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 2) as usize;
            if idx + 1 < pixels.len() {
                // RGB565: RRRRR GGGGGG BBBBB (16-bit little-endian)
                let lo = pixels[idx] as u16;
                let hi = pixels[idx + 1] as u16;
                let rgb565 = lo | (hi << 8);

                // Extract and expand to 8-bit
                let r = ((rgb565 >> 11) & 0x1F) as u8;
                let g = ((rgb565 >> 5) & 0x3F) as u8;
                let b = (rgb565 & 0x1F) as u8;

                // Scale 5-bit/6-bit to 8-bit
                rgb_data.push((r << 3) | (r >> 2)); // R: 5-bit to 8-bit
                rgb_data.push((g << 2) | (g >> 4)); // G: 6-bit to 8-bit
                rgb_data.push((b << 3) | (b >> 2)); // B: 5-bit to 8-bit
            } else {
                // Fill with black if out of bounds
                rgb_data.push(0);
                rgb_data.push(0);
                rgb_data.push(0);
            }
        }
    }

    file.write_all(&rgb_data)?;
    println!("Screen dump saved to: {}", filename);
    Ok(())
}

/// Run a Glide API test with automatic window and event loop management
///
/// # Arguments
/// * `title` - Window title
/// * `setup` - Called once after Glide is initialized (for state setup)
/// * `render` - Called each frame to render content
///
/// # Example
/// ```ignore
/// run_test(
///     "test01 - Red Triangle",
///     |_config| {
///         // One-time setup (e.g., grDepthBufferMode)
///     },
///     |config| {
///         grBufferClear(Color(0x00000000), Alpha(0), 0xFFFF);
///         // Draw triangles...
///         grBufferSwap(1);
///     },
/// );
/// ```
pub fn run_test<S, R>(title: &str, setup: S, mut render: R)
where
    S: FnOnce(&TestConfig),
    R: FnMut(&TestConfig) + 'static,
{
    env_logger::init();

    let config = match parse_args() {
        Some(c) => c,
        None => return,
    };

    println!("{}:", title);
    println!("Resolution: {}x{}", config.width, config.height);
    if config.frames > 0 {
        println!("Rendering {} frames", config.frames);
    } else {
        println!("Close window to quit");
    }

    let event_loop = EventLoop::new().expect("Failed to create event loop");

    let window = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(winit::dpi::LogicalSize::new(config.width, config.height))
        .with_visible(!config.headless)
        .build(&event_loop)
        .expect("Failed to create window");

    // Initialize Glide with the window
    grGlideInitWithWindow(window);

    // Select SST and open window
    grSstSelect(0);
    let ctx = grSstWinOpen(
        0,
        config.resolution,
        ScreenRefresh::R60Hz,
        ColorFormat::Abgr,
        OriginLocation::UpperLeft,
        2,
        1,
    );
    assert_eq!(ctx, 1, "Failed to open graphics context");

    // Call user's setup function
    setup(&config);

    let mut frame_count: u32 = 0;
    let mut frames_remaining = config.frames;
    let dump_file = config.dump_file.clone();
    let mut shutdown = false;

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);

            // Don't process events after shutdown
            if shutdown {
                return;
            }

            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => {
                        println!("Shutting down after {} frames", frame_count);
                        shutdown = true;
                        grGlideShutdown();
                        elwt.exit();
                    }
                    WindowEvent::RedrawRequested => {
                        if frames_remaining == 0 {
                            println!("Completed {} frames", frame_count);
                            shutdown = true;
                            grGlideShutdown();
                            elwt.exit();
                            return;
                        }

                        // Call user's render function
                        render(&config);

                        // Screen dump if requested
                        if let Some(ref filename) = dump_file {
                            // Get actual framebuffer dimensions (may differ from logical size on HiDPI)
                            let width = grSstScreenWidth();
                            let height = grSstScreenHeight();
                            let stride = width * 2; // RGB565 = 2 bytes per pixel
                            let mut pixels = vec![0u8; (width * height * 2) as usize];

                            if grLfbReadRegion(
                                Buffer::Back,
                                0,
                                0,
                                width,
                                height,
                                stride,
                                pixels.as_mut_ptr() as *mut _,
                            ) == FxBool::True
                            {
                                if let Err(e) = screen_dump(filename, width, height, &pixels) {
                                    eprintln!("Failed to save screen dump: {}", e);
                                }
                            } else {
                                eprintln!("Failed to read framebuffer");
                            }
                        }

                        frame_count += 1;
                        if frames_remaining > 0 {
                            frames_remaining -= 1;
                        }
                    }
                    _ => {}
                },
                Event::AboutToWait => {
                    grGetWindow(|window| window.request_redraw());
                }
                _ => {}
            }
        })
        .unwrap();
}

/// Run a Glide API test with keyboard input support
///
/// Similar to `run_test` but the render callback receives a slice of characters
/// that were pressed since the last frame. The callback returns `true` to continue
/// or `false` to exit.
///
/// # Arguments
/// * `title` - Window title
/// * `setup` - Called once after Glide is initialized (for state setup)
/// * `render` - Called each frame with config and pressed keys; returns `true` to continue
///
/// # Example
/// ```ignore
/// run_test_interactive(
///     "test12 - Interactive Test",
///     |_config| {
///         // One-time setup
///     },
///     |config, keys| {
///         for key in keys {
///             match key {
///                 '1' => toggle_option_1(),
///                 '2' => toggle_option_2(),
///                 _ => {}
///             }
///         }
///         grBufferClear(Color(0x00000000), Alpha(0), 0xFFFF);
///         // Draw...
///         grBufferSwap(1);
///         true // continue running
///     },
/// );
/// ```
pub fn run_test_interactive<S, R>(title: &str, setup: S, mut render: R)
where
    S: FnOnce(&TestConfig),
    R: FnMut(&TestConfig, &[char]) -> bool + 'static,
{
    env_logger::init();

    let config = match parse_args() {
        Some(c) => c,
        None => return,
    };

    println!("{}:", title);
    println!("Resolution: {}x{}", config.width, config.height);
    if config.frames > 0 {
        println!("Rendering {} frames", config.frames);
    } else {
        println!("Close window to quit");
    }

    let event_loop = EventLoop::new().expect("Failed to create event loop");

    let window = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(winit::dpi::LogicalSize::new(config.width, config.height))
        .with_visible(!config.headless)
        .build(&event_loop)
        .expect("Failed to create window");

    // Initialize Glide with the window
    grGlideInitWithWindow(window);

    // Select SST and open window
    grSstSelect(0);
    let ctx = grSstWinOpen(
        0,
        config.resolution,
        ScreenRefresh::R60Hz,
        ColorFormat::Abgr,
        OriginLocation::UpperLeft,
        2,
        1,
    );
    assert_eq!(ctx, 1, "Failed to open graphics context");

    // Call user's setup function
    setup(&config);

    let mut frame_count: u32 = 0;
    let mut frames_remaining = config.frames;
    let dump_file = config.dump_file.clone();
    let mut shutdown = false;
    let mut pressed_keys: Vec<char> = Vec::new();

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);

            // Don't process events after shutdown
            if shutdown {
                return;
            }

            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => {
                        println!("Shutting down after {} frames", frame_count);
                        shutdown = true;
                        grGlideShutdown();
                        elwt.exit();
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        if event.state == ElementState::Pressed && !event.repeat {
                            // Convert key to character
                            match &event.logical_key {
                                Key::Character(c) => {
                                    // Get first character from the string
                                    if let Some(ch) = c.chars().next() {
                                        pressed_keys.push(ch);
                                    }
                                }
                                Key::Named(NamedKey::Escape) => {
                                    // ESC quits
                                    println!(
                                        "Escape pressed, shutting down after {} frames",
                                        frame_count
                                    );
                                    shutdown = true;
                                    grGlideShutdown();
                                    elwt.exit();
                                }
                                Key::Named(NamedKey::Space) => {
                                    pressed_keys.push(' ');
                                }
                                _ => {}
                            }
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        if frames_remaining == 0 {
                            println!("Completed {} frames", frame_count);
                            shutdown = true;
                            grGlideShutdown();
                            elwt.exit();
                            return;
                        }

                        // Call user's render function with pressed keys
                        // Returns false to exit
                        if !render(&config, &pressed_keys) {
                            println!("Exiting after {} frames", frame_count);
                            shutdown = true;
                            grGlideShutdown();
                            elwt.exit();
                            pressed_keys.clear();
                            return;
                        }
                        pressed_keys.clear();

                        // Screen dump if requested
                        if let Some(ref filename) = dump_file {
                            // Get actual framebuffer dimensions (may differ from logical size on HiDPI)
                            let width = grSstScreenWidth();
                            let height = grSstScreenHeight();
                            let stride = width * 2; // RGB565 = 2 bytes per pixel
                            let mut pixels = vec![0u8; (width * height * 2) as usize];

                            if grLfbReadRegion(
                                Buffer::Back,
                                0,
                                0,
                                width,
                                height,
                                stride,
                                pixels.as_mut_ptr() as *mut _,
                            ) == FxBool::True
                            {
                                if let Err(e) = screen_dump(filename, width, height, &pixels) {
                                    eprintln!("Failed to save screen dump: {}", e);
                                }
                            } else {
                                eprintln!("Failed to read framebuffer");
                            }
                        }

                        frame_count += 1;
                        if frames_remaining > 0 {
                            frames_remaining -= 1;
                        }
                    }
                    _ => {}
                },
                Event::AboutToWait => {
                    grGetWindow(|window| window.request_redraw());
                }
                _ => {}
            }
        })
        .unwrap();
}
