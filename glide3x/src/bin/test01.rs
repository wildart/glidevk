//! test01 - Draws a diagonal line of points from top-left to bottom-right
//!
//! This is a Rust port of the original Glide test01.c
//!
//! Usage: test01 [-n <frames>] [-r <resolution>] [-d <filename>]
//!   -n <frames>     Number of frames to render (-1 for infinite, default)
//!   -r <resolution> Screen resolution (e.g., 640x480, 800x600, 1024x768)
//!   -d <filename>   Dump screen to PPM file

use glide3x::testutil;
use glide3x::{
    grBufferClear, grBufferSwap, grColorCombine, grConstantColorValue, grDrawPoint, grVertexLayout,
    Alpha, Color, CombineFactor, CombineFunction, CombineLocal, CombineOther, FxBool, Vertex,
};

// Vertex layout parameters (from glide.h)
const GR_PARAM_XY: u32 = 0x01;
const GR_PARAM_ENABLE: u32 = 0x01;

fn main() {
    testutil::run_test_interactive(
        "test01 - Diagonal points (press any key to exit)",
        |_config| {
            // Set up vertex layout (XY coordinates enabled)
            grVertexLayout(GR_PARAM_XY, 0, GR_PARAM_ENABLE);

            // Set up render state - flat shading with constant color
            grColorCombine(
                CombineFunction::Local,
                CombineFactor::Zero,
                CombineLocal::Constant,
                CombineOther::Iterated,
                FxBool::False,
            );

            // Set constant color to white (0xFFFFFFFF = opaque white in ARGB)
            // Original test uses 0xFFFFFF but constant color is always ARGB format
            grConstantColorValue(Color(0xFFFFFFFF));

            println!("Press any key to exit");
        },
        |config, keys| {
            // Clear to black with max depth
            grBufferClear(Color(0x00000000), Alpha(0), 0xFFFF);

            // Draw 100 points diagonally from top-left to bottom-right
            for i in 0..100 {
                let pos = (i as f32) / 100.0;
                let vtx = Vertex::new(config.scale_x(pos), config.scale_y(pos));
                grDrawPoint(&vtx);
            }

            grBufferSwap(1);

            // Exit if any key pressed
            keys.is_empty()
        },
    );
}
