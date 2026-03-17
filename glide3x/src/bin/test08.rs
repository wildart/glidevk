//! test08 - Fogging test
//!
//! This is a Rust port of the original Glide test08.c
//! Renders a triangle with fog effect based on distance
//!
//! Usage: test08 [-n <frames>] [-r <resolution>]

use glide3x::testutil;
use glide3x::{
    grBufferClear, grBufferSwap, grConstantColorValue, grDrawTriangle, grFogColorValue, grFogMode,
    Alpha, Color, FogMode, Vertex,
};

fn main() {
    testutil::run_test(
        "test08 - Fogging",
        |_config| {
            // Set up fog
            grFogMode(FogMode::WithTableOnW);
            grFogColorValue(Color(0xFF00FF00)); // Green fog
                                                // Note: fog_table not yet implemented, fog effect may not be visible
        },
        |config| {
            grBufferClear(Color(0x00000000), Alpha(0), 0xFFFF);

            // Triangle with varying w distances (fog increases with distance)
            // Near vertex (w=20, oow=0.05) - less fog
            let mut vtx_a = Vertex::new(config.scale_x(0.0), config.scale_y(0.0));
            vtx_a.r = 0.0;
            vtx_a.g = 0.0;
            vtx_a.b = 64.0;
            vtx_a.oow = 1.0 / 20.0;

            // Far vertex (w=2000, oow=0.0005) - more fog
            let mut vtx_b = Vertex::new(config.scale_x(0.016), config.scale_y(1.0));
            vtx_b.r = 0.0;
            vtx_b.g = 0.0;
            vtx_b.b = 128.0;
            vtx_b.oow = 1.0 / 2000.0;

            // Near vertex (w=20) - less fog
            let mut vtx_c = Vertex::new(config.scale_x(1.0), config.scale_y(0.0208));
            vtx_c.r = 0.0;
            vtx_c.g = 0.0;
            vtx_c.b = 64.0;
            vtx_c.oow = 1.0 / 20.0;

            // Set a blue color for the triangle (fog effect not yet implemented)
            // ABGR: A=FF, B=80, G=00, R=00 = dark blue with full alpha
            grConstantColorValue(Color(0xFF800000));
            grDrawTriangle(&vtx_a, &vtx_b, &vtx_c);

            grBufferSwap(1);
        },
    );
}
