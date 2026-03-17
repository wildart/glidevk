//! test10 - Culling test
//!
//! This is a Rust port of the original Glide test10.c
//! Renders red and blue triangles with different winding orders,
//! culls one based on cull mode
//!
//! Usage: test10 [-n <frames>] [-r <resolution>]

use glide3x::testutil;
use glide3x::{
    grBufferClear, grBufferSwap, grConstantColorValue, grCullMode, grDrawTriangle, Alpha, Color,
    CullMode, Vertex,
};

fn main() {
    testutil::run_test(
        "test10 - Culling",
        |_config| {
            // Cull positive winding (red triangle culled, blue visible)
            grCullMode(CullMode::Positive);
            println!("Cull mode: POSITIVE (red triangle with clockwise winding should be culled)");
        },
        |config| {
            grBufferClear(Color(0x00000000), Alpha(0), 0);

            // Red triangle with positive/clockwise winding (A->B->C)
            // Should be culled when cull mode is POSITIVE
            let vtx_a = Vertex::new(config.scale_x(0.5), config.scale_y(0.1));
            let vtx_b = Vertex::new(config.scale_x(0.8), config.scale_y(0.5));
            let vtx_c = Vertex::new(config.scale_x(0.2), config.scale_y(0.5));

            // Red with full alpha: ABGR = A:FF, B:00, G:00, R:FF
            grConstantColorValue(Color(0xFF0000FF));
            grDrawTriangle(&vtx_a, &vtx_b, &vtx_c);

            // Blue triangle with negative/counter-clockwise winding (A->B->C)
            // Should be visible when cull mode is POSITIVE
            let vtx_a2 = Vertex::new(config.scale_x(0.5), config.scale_y(0.9));
            let vtx_b2 = Vertex::new(config.scale_x(0.8), config.scale_y(0.5));
            let vtx_c2 = Vertex::new(config.scale_x(0.2), config.scale_y(0.5));

            // Blue with full alpha: ABGR = A:FF, B:FF, G:00, R:00
            grConstantColorValue(Color(0xFFFF0000));
            grDrawTriangle(&vtx_a2, &vtx_b2, &vtx_c2); // CCW winding produces negative area

            grBufferSwap(1);
        },
    );
}
