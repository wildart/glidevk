//! test12 - LFB write modes test
//!
//! This is a Rust port of the original Glide test12.c
//! Tests LFB write modes, render buffer, pixel pipeline, and Y origin.
//!
//! Keyboard controls:
//! - 1: Toggle LFB Y origin (upper left / lower left)
//! - 2: Toggle Glide Y origin
//! - 3: Toggle LFB render buffer (front/back)
//! - 4: Toggle Glide render buffer
//! - 5: Toggle pixel pipeline
//! - 6: Cycle write mode (565, 555, 1555, 8888)
//! - ESC or any other key: Quit
//!
//! Usage: test12 [-n <frames>] [-r <resolution>] [-d <filename>]

use glide3x::testutil;
use glide3x::{
    grBufferClear, grBufferSwap, grLfbLock, grLfbUnlock, grRenderBuffer, grSstOrigin, Alpha,
    Buffer, Color, FxBool, LfbInfo, LfbLock, LfbWriteMode, OriginLocation,
};

/// Generate a 256x256 RGB565 texture pattern (similar to decal1.3df)
fn generate_texture() -> Vec<u16> {
    let mut data = vec![0u16; 256 * 256];
    for y in 0..256 {
        for x in 0..256 {
            // Create a colorful gradient pattern
            let r = x as u8;
            let g = y as u8;
            let b = ((x + y) / 2) as u8;
            // RGB565: RRRRR GGGGGG BBBBB
            data[y * 256 + x] =
                ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | ((b as u16 & 0xF8) >> 3);
        }
    }
    data
}

/// Convert RGB565 to RGB555 format
fn rgb565_to_rgb555(pixel: u16) -> u16 {
    // RGB565: RRRRR GGGGGG BBBBB -> RGB555: 1RRRRR GGGGG BBBBB
    0x8000 | ((pixel >> 1) & 0x7C00) | ((pixel >> 1) & 0x03E0) | (pixel & 0x1F)
}

/// Convert RGB565 to ARGB8888 format
fn rgb565_to_argb8888(pixel: u16) -> u32 {
    let r = ((pixel >> 11) & 0x1F) as u32;
    let g = ((pixel >> 5) & 0x3F) as u32;
    let b = (pixel & 0x1F) as u32;
    // Expand to 8-bit and pack as ARGB8888
    0xFF000000 | ((r << 3) << 16) | ((g << 2) << 8) | (b << 3)
}

fn write_mode_name(mode: LfbWriteMode) -> &'static str {
    match mode {
        LfbWriteMode::Rgb565 => "RGB565",
        LfbWriteMode::Rgb555 => "RGB555",
        LfbWriteMode::Argb1555 => "ARGB1555",
        LfbWriteMode::Rgb888 => "RGB888",
        LfbWriteMode::Argb8888 => "ARGB8888",
        _ => "RESERVED",
    }
}

fn origin_name(origin: OriginLocation) -> &'static str {
    match origin {
        OriginLocation::UpperLeft => "UPPER_LEFT",
        OriginLocation::LowerLeft => "LOWER_LEFT",
        _ => "ANY",
    }
}

fn buffer_name(buffer: Buffer) -> &'static str {
    match buffer {
        Buffer::Back => "BACKBUFFER",
        Buffer::Front => "FRONTBUFFER",
        _ => "OTHER",
    }
}

/// Write texture to framebuffer via LFB lock with specified parameters
fn write_texture_to_buffer(
    lfb_buffer: Buffer,
    write_mode: LfbWriteMode,
    lfb_origin: OriginLocation,
    pixel_pipe: FxBool,
    texture_data: &[u16],
    start_x: usize,
    start_y: usize,
    max_width: usize,
    max_height: usize,
) -> bool {
    let mut info = LfbInfo::default();

    // Lock the specified buffer with given parameters
    if grLfbLock(
        LfbLock::WriteOnly,
        lfb_buffer,
        write_mode,
        lfb_origin,
        pixel_pipe,
        &mut info,
    ) == FxBool::True
    {
        let stride = info.stride_in_bytes as usize;
        let lfb_ptr = info.lfb_ptr as *mut u8;

        // Write the 256x256 texture to the framebuffer
        for y in 0..256 {
            for x in 0..256 {
                // Bounds check
                if start_y + y < max_height && start_x + x < max_width {
                    let src_pixel = texture_data[y * 256 + x];

                    match write_mode {
                        LfbWriteMode::Rgb565 => {
                            let pixel_offset = (start_y + y) * stride + (start_x + x) * 2;
                            unsafe {
                                let pixel_ptr = lfb_ptr.add(pixel_offset) as *mut u16;
                                *pixel_ptr = src_pixel;
                            }
                        }
                        LfbWriteMode::Rgb555 | LfbWriteMode::Argb1555 => {
                            let pixel_offset = (start_y + y) * stride + (start_x + x) * 2;
                            let converted = rgb565_to_rgb555(src_pixel);
                            unsafe {
                                let pixel_ptr = lfb_ptr.add(pixel_offset) as *mut u16;
                                *pixel_ptr = converted;
                            }
                        }
                        LfbWriteMode::Rgb888 | LfbWriteMode::Argb8888 => {
                            let pixel_offset = (start_y + y) * stride + (start_x + x) * 4;
                            let converted = rgb565_to_argb8888(src_pixel);
                            unsafe {
                                let pixel_ptr = lfb_ptr.add(pixel_offset) as *mut u32;
                                *pixel_ptr = converted;
                            }
                        }
                        _ => {
                            // For other modes, use RGB565
                            let pixel_offset = (start_y + y) * stride + (start_x + x) * 2;
                            unsafe {
                                let pixel_ptr = lfb_ptr.add(pixel_offset) as *mut u16;
                                *pixel_ptr = src_pixel;
                            }
                        }
                    }
                }
            }
        }

        // Unlock the buffer
        grLfbUnlock(LfbLock::WriteOnly, lfb_buffer);
        true
    } else {
        false
    }
}

fn main() {
    // Generate texture data (256x256 RGB565)
    let texture_data = generate_texture();

    // Configuration state
    let mut lfb_origin = OriginLocation::UpperLeft;
    let mut sst_origin = OriginLocation::UpperLeft;
    let mut lfb_buffer = Buffer::Back;
    let mut sst_buffer = Buffer::Back;
    let mut pixel_pipe = FxBool::False;
    let mut write_mode = LfbWriteMode::Rgb565;

    // Supported write modes to cycle through
    let write_modes = [
        LfbWriteMode::Rgb565,
        LfbWriteMode::Rgb555,
        LfbWriteMode::Argb1555,
        LfbWriteMode::Argb8888,
    ];
    let mut write_mode_idx: usize = 0;

    testutil::run_test_interactive(
        "test12 - LFB Write Modes Test",
        |_config| {
            println!("LFB write modes test - interactive keyboard controls:");
            println!("  1: Toggle LFB Y origin");
            println!("  2: Toggle Glide Y origin");
            println!("  3: Toggle LFB render buffer (front/back)");
            println!("  4: Toggle Glide render buffer");
            println!("  5: Toggle pixel pipeline");
            println!("  6: Cycle write mode");
            println!("  ESC: Quit");
        },
        move |config, keys| {
            // Process keyboard input
            for &key in keys {
                match key {
                    '1' => {
                        // Toggle LFB Y origin
                        lfb_origin = if lfb_origin == OriginLocation::UpperLeft {
                            OriginLocation::LowerLeft
                        } else {
                            OriginLocation::UpperLeft
                        };
                        println!("LFB origin: {}", origin_name(lfb_origin));
                    }
                    '2' => {
                        // Toggle Glide Y origin
                        sst_origin = if sst_origin == OriginLocation::UpperLeft {
                            OriginLocation::LowerLeft
                        } else {
                            OriginLocation::UpperLeft
                        };
                        grSstOrigin(sst_origin);
                        println!("Glide origin: {}", origin_name(sst_origin));
                    }
                    '3' => {
                        // Toggle LFB render buffer
                        lfb_buffer = if lfb_buffer == Buffer::Back {
                            Buffer::Front
                        } else {
                            Buffer::Back
                        };
                        println!("LFB buffer: {}", buffer_name(lfb_buffer));
                    }
                    '4' => {
                        // Toggle Glide render buffer
                        sst_buffer = if sst_buffer == Buffer::Back {
                            Buffer::Front
                        } else {
                            Buffer::Back
                        };
                        println!("Glide buffer: {}", buffer_name(sst_buffer));
                    }
                    '5' => {
                        // Toggle pixel pipeline
                        pixel_pipe = if pixel_pipe == FxBool::False {
                            FxBool::True
                        } else {
                            FxBool::False
                        };
                        println!(
                            "Pixel pipeline: {}",
                            if pixel_pipe == FxBool::True {
                                "ON"
                            } else {
                                "OFF"
                            }
                        );
                    }
                    '6' => {
                        // Cycle write mode
                        write_mode_idx = (write_mode_idx + 1) % write_modes.len();
                        write_mode = write_modes[write_mode_idx];
                        println!("Write mode: {}", write_mode_name(write_mode));
                    }
                    _ => {}
                }
            }

            // Clear both buffers first
            grRenderBuffer(Buffer::Back);
            grBufferClear(Color(0x00000000), Alpha(0), 0xFFFF);
            grRenderBuffer(Buffer::Front);
            grBufferClear(Color(0x00000000), Alpha(0), 0xFFFF);

            // Set Glide render buffer and origin
            grRenderBuffer(sst_buffer);
            grSstOrigin(sst_origin);

            // Calculate start position (centered)
            let start_x = ((config.width as i32 - 256) / 2).max(0) as usize;
            let start_y = ((config.height as i32 - 256) / 2).max(0) as usize;

            // Write texture with current configuration
            if !write_texture_to_buffer(
                lfb_buffer,
                write_mode,
                lfb_origin,
                pixel_pipe,
                &texture_data,
                start_x,
                start_y,
                config.width as usize,
                config.height as usize,
            ) {
                println!("Lock failed for this configuration");
            }

            // Swap to display
            grBufferSwap(1);

            true // continue running
        },
    );
}
