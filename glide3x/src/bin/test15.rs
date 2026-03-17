//! test15 - Clip rectangle test
//!
//! This is a Rust port of the original Glide test15.c
//! Tests grClipWindow by drawing a 10x10 grid of triangles
//! with a clip rectangle that shows only a portion of the grid.
//!
//! Usage: test15 [-n <frames>] [-r <resolution>] [-d <filename>]

use glide3x::testutil;
use glide3x::{
    grBufferClear, grBufferSwap, grClipWindow, grColorCombine, grConstantColorValue,
    grDrawTriangle, grFinish, grGet, grVertexLayout, Alpha, Color, CombineFactor, CombineFunction,
    CombineLocal, CombineOther, FxBool, FxI32, GrGetParam, Vertex,
};

// Vertex layout parameters (from glide.h)
const GR_PARAM_XY: u32 = 0x01;
const GR_PARAM_ENABLE: u32 = 0x01;

fn main() {
    testutil::run_test(
        "test15 - Clip Rectangle",
        |_config| {
            // Get Z depth min/max values
            let mut zrange: [FxI32; 2] = [0; 2];
            grGet(GrGetParam::ZdepthMinMax, 8, zrange.as_mut_ptr());
            // zrange[0] = min (0), zrange[1] = max (0xFFFF)

            // Set up vertex layout
            grVertexLayout(GR_PARAM_XY, 0, GR_PARAM_ENABLE);

            // Set up render state - constant color
            grColorCombine(
                CombineFunction::Local,
                CombineFactor::Zero,
                CombineLocal::Constant,
                CombineOther::Constant,
                FxBool::False,
            );

            println!("Clip rectangle test - grid of triangles with clip window");
        },
        |config| {
            // First clear with full clip window
            grClipWindow(0, 0, config.width, config.height);
            grBufferClear(Color(0x00000000), Alpha(0), 0xFFFF); // Black background, max z depth

            // Set clip rectangle to show only center portion
            let clip_x = 0.2;
            let clip_y = 0.2;
            let clip_size = 0.6;

            let minx = (clip_x * config.width as f32) as u32;
            let miny = (clip_y * config.height as f32) as u32;
            let maxx = ((clip_x + clip_size) * config.width as f32) as u32;
            let maxy = ((clip_y + clip_size) * config.height as f32) as u32;

            grClipWindow(minx, miny, maxx, maxy);

            // Draw 10x10 grid of triangles
            // Gray: ABGR = A:FF, B:80, G:80, R:80
            grConstantColorValue(Color(0xFF808080));

            for y in 0..10 {
                for x in 0..10 {
                    // Each cell is 0.1 x 0.1 of the screen
                    let x0 = x as f32 / 10.0;
                    let y0 = y as f32 / 10.0;
                    let y1 = (y as f32 + 1.0) / 10.0;
                    let x1 = (x as f32 + 1.0) / 10.0;

                    /*
                       A
                       |\
                       B-C
                    */
                    let vtx_a = Vertex::new(config.scale_x(x0), config.scale_y(y0));
                    let vtx_b = Vertex::new(config.scale_x(x0), config.scale_y(y1));
                    let vtx_c = Vertex::new(config.scale_x(x1), config.scale_y(y1));

                    grDrawTriangle(&vtx_a, &vtx_b, &vtx_c);
                }
            }

            // Reset clip window before swap (some implementations need this)
            grClipWindow(0, 0, config.width, config.height);

            grBufferSwap(1);
            grFinish();
        },
    );
}
