//! test11 - LFB read/write test
//!
//! This is a Rust port of the original Glide test11.c
//! Tests linear frame buffer read/write functionality by:
//! 1. Creating a 64x64 color pattern in memory
//! 2. Writing it to the back buffer via grLfbLock (write)
//! 3. Swapping buffers
//! 4. Reading it back via grLfbLock (read)
//! 5. Comparing the written and read data
//!
//! Usage: test11 [-n <frames>] [-r <resolution>] [-d <filename>]

use glide3x::testutil;
use glide3x::{
    grBufferClear, grBufferSwap, grDitherMode, grLfbLock, grLfbUnlock, Alpha, Buffer, Color,
    DitherMode, FxBool, LfbInfo, LfbLock, LfbWriteMode, OriginLocation,
};

fn main() {
    // Create source bitmap (64x64 pixels, RGB565 format)
    let mut color_buf: [[u16; 64]; 64] = [[0; 64]; 64];
    for y in 0..64 {
        for x in 0..64 {
            let red = (x << 2) as u8;
            let grn = (y << 2) as u8;
            let blu = ((x + y) << 1) as u8;
            // RGB565: RRRRR GGGGGG BBBBB
            color_buf[y][x] = ((red as u16 & 0xF8) << 8)
                | ((grn as u16 & 0xFC) << 3)
                | ((blu as u16 & 0xF8) >> 3);
        }
    }

    let color_buf_copy = color_buf; // Keep a copy for comparison

    testutil::run_test(
        "test11 - LFB Read/Write Test",
        |_config| {
            // Disable dithering for accurate pixel comparisons
            grDitherMode(DitherMode::Disable);

            println!("LFB read/write test - writes pattern, reads back, compares");
        },
        move |config| {
            grBufferClear(Color(0x00000000), Alpha(0), 0xFFFF);

            // Prepare info structure for lock
            let mut info: LfbInfo = LfbInfo::default();

            // Lock back buffer for writing
            if grLfbLock(
                LfbLock::WriteOnly,
                Buffer::Back,
                LfbWriteMode::Rgb565,
                OriginLocation::UpperLeft,
                FxBool::False,
                &mut info,
            ) == FxBool::False
            {
                println!("Error: Failed to take write lock");
                grBufferSwap(1);
                return;
            }

            // Calculate start position (centered with some margin)
            let start_x = ((config.width as i32 - 64) / 2).max(0) as usize;
            let start_y = ((config.height as i32 - 64) / 2).max(0) as usize;

            // Write the color pattern to the framebuffer
            let stride = info.stride_in_bytes as usize;
            let lfb_ptr = info.lfb_ptr as *mut u8;

            for y in 0..64 {
                for x in 0..64 {
                    let pixel_offset = (start_y + y) * stride + (start_x + x) * 2;
                    unsafe {
                        let pixel_ptr = lfb_ptr.add(pixel_offset) as *mut u16;
                        *pixel_ptr = color_buf[y][x];
                    }
                }
            }

            // Unlock the back buffer
            grLfbUnlock(LfbLock::WriteOnly, Buffer::Back);

            // Swap to make it visible
            grBufferSwap(1);

            // Now read it back and compare
            grBufferClear(Color(0x00000000), Alpha(0), 0xFFFF);

            // Lock front buffer for reading
            let mut read_info = LfbInfo::default();
            if grLfbLock(
                LfbLock::ReadOnly,
                Buffer::Front,
                LfbWriteMode::Any,
                OriginLocation::UpperLeft,
                FxBool::False,
                &mut read_info,
            ) == FxBool::False
            {
                println!("Error: Failed to take read lock");
                grBufferSwap(1);
                return;
            }

            // Read back the pattern
            let mut grab_buf: [[u16; 64]; 64] = [[0; 64]; 64];
            let read_stride = read_info.stride_in_bytes as usize;
            let read_ptr = read_info.lfb_ptr as *const u8;

            for y in 0..64 {
                for x in 0..64 {
                    let pixel_offset = (start_y + y) * read_stride + (start_x + x) * 2;
                    unsafe {
                        let pixel_ptr = read_ptr.add(pixel_offset) as *const u16;
                        grab_buf[y][x] = *pixel_ptr;
                    }
                }
            }

            // Unlock the front buffer
            grLfbUnlock(LfbLock::ReadOnly, Buffer::Front);

            // Compare the source image to the readback image
            let mut matches = true;
            for y in 0..64 {
                for x in 0..64 {
                    if color_buf_copy[y][x] != grab_buf[y][x] {
                        matches = false;
                        break;
                    }
                }
                if !matches {
                    break;
                }
            }

            if matches {
                println!("Passed readback test");
            } else {
                println!("Failed readback test");
            }

            grBufferSwap(1);
        },
    );
}
