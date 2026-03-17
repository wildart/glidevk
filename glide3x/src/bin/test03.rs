//! test03 - Draws gouraud shaded lines
//!
//! This is a Rust port of the original Glide test03.c
//! Draws a diagonal line from top-left (red) to bottom-right (green)
//!
//! Usage: test03 [-n <frames>] [-r <resolution>]

use glide3x::testutil;
use glide3x::{grBufferClear, grBufferSwap, grDrawLine, Alpha, Color, Vertex};

fn main() {
    testutil::run_test(
        "test03 - Gouraud shaded line",
        |_config| {},
        |config| {
            grBufferClear(Color(0x00000000), Alpha(0), 0xFFFF);

            // Draw gouraud shaded line from (0,0) red to (1,1) green
            let mut vtx_a = Vertex::new(0.0, 0.0);
            vtx_a.r = 255.0;
            vtx_a.g = 0.0;
            vtx_a.b = 0.0;
            vtx_a.a = 255.0;

            let mut vtx_b = Vertex::new(config.width_f(), config.height_f());
            vtx_b.r = 0.0;
            vtx_b.g = 255.0;
            vtx_b.b = 0.0;
            vtx_b.a = 255.0;

            grDrawLine(&vtx_a, &vtx_b);

            grBufferSwap(1);
        },
    );
}
