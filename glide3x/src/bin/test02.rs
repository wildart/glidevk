//! test02 - Draws a parabolic envelope of lines
//!
//! This is a Rust port of the original Glide test02.c
//!
//! Usage: test02 [-n <frames>] [-r <resolution>]

use glide3x::testutil;
use glide3x::{
    grBufferClear, grBufferSwap, grConstantColorValue, grDrawLine, Alpha, Color, Vertex,
};

fn main() {
    testutil::run_test(
        "test02 - Parabolic envelope",
        |_config| {
            grConstantColorValue(Color(0xFFFFFFFF));
        },
        |config| {
            grBufferClear(Color(0x00000000), Alpha(0), 0xFFFF);

            // Draw 100 lines forming a parabolic envelope
            for i in 0..100 {
                let pos = (i as f32) / 100.0;

                let mut vtx_a = Vertex::new(config.scale_x(pos), 0.0);
                vtx_a.r = 255.0;
                vtx_a.g = 255.0;
                vtx_a.b = 255.0;
                vtx_a.a = 255.0;

                let mut vtx_b = Vertex::new(config.width_f(), config.scale_y(pos));
                vtx_b.r = 255.0;
                vtx_b.g = 255.0;
                vtx_b.b = 255.0;
                vtx_b.a = 255.0;

                grDrawLine(&vtx_a, &vtx_b);
            }

            grBufferSwap(1);
        },
    );
}
