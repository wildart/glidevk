//! test00 - Clear screen to blue
//!
//! This is a Rust port of the original Glide test00.c
//! It simply clears the screen to blue and waits for any key to exit.
//!
//! Usage: test00 [-n <frames>] [-r <resolution>]
//!   -n <frames>     Number of frames to render (-1 for infinite, default)
//!   -r <resolution> Screen resolution (e.g., 640x480, 800x600, 1024x768)

use glide3x::testutil;
use glide3x::{
    grBufferClear, grBufferSwap, grGet, grGetString, Alpha, Color, EnvironmentInfo, FxI32,
    GrGetParam,
};

fn main() {
    // W depth range - populated in setup, used in render
    use std::cell::Cell;
    use std::rc::Rc;
    let wrange = Rc::new(Cell::new(0xFFFFu32));
    let wrange_render = Rc::clone(&wrange);

    testutil::run_test_interactive(
        "test00 - Clear screen to blue (press any key to exit)",
        move |_config| {
            // Get and print Glide version
            let version = grGetString(EnvironmentInfo::Version);
            let version_str = unsafe { std::ffi::CStr::from_ptr(version) };
            println!("Glide version: {}", version_str.to_string_lossy());

            // Get W depth min/max values
            let mut range: [FxI32; 2] = [0; 2];
            grGet(GrGetParam::WdepthMinMax, 8, range.as_mut_ptr());
            wrange.set(range[1] as u32);

            println!("Press any key to exit");
        },
        move |_config, keys| {
            // Clear to blue (ABGR: A=00, B=FF, G=00, R=00)
            grBufferClear(Color(0x00FF0000), Alpha(0), wrange_render.get());
            grBufferSwap(1);

            // Exit if any key pressed
            keys.is_empty()
        },
    );
}
