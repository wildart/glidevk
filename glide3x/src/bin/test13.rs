//! test13 - Iterated alpha test
//!
//! This is a Rust port of the original Glide test13.c
//! Renders a red triangle and a blue triangle that fades towards one vertex
//! using iterated (per-vertex) alpha values.
//!
//! Usage: test13 [-n <frames>] [-r <resolution>] [-d <filename>]

use glide3x::testutil;
use glide3x::{
    grAlphaBlendFunction, grBufferClear, grBufferSwap, grConstantColorValue, grDrawTriangle, Alpha,
    AlphaBlendFnc, Color, Vertex,
};

fn main() {
    testutil::run_test(
        "test13 - Iterated Alpha",
        |_config| {
            // Enable alpha blending: src * srcAlpha + dst * (1 - srcAlpha)
            grAlphaBlendFunction(
                AlphaBlendFnc::SrcAlpha,
                AlphaBlendFnc::OneMinusSrcAlpha,
                AlphaBlendFnc::Zero,
                AlphaBlendFnc::Zero,
            );
            println!("Iterated alpha test - blue triangle fades towards top vertex");
        },
        |config| {
            grBufferClear(Color(0xFF000000), Alpha(0), 0xFFFF);

            // Red triangle (background) - fully opaque
            let mut vtx_a = Vertex::new(config.scale_x(0.5), config.scale_y(0.1));
            let mut vtx_b = Vertex::new(config.scale_x(0.8), config.scale_y(0.9));
            let mut vtx_c = Vertex::new(config.scale_x(0.2), config.scale_y(0.9));
            vtx_a.a = 255.0;
            vtx_b.a = 255.0;
            vtx_c.a = 255.0;

            // Red: ABGR = A:FF, B:00, G:00, R:FF
            grConstantColorValue(Color(0xFF0000FF));
            grDrawTriangle(&vtx_a, &vtx_b, &vtx_c);

            // Blue triangle (foreground) - fades from transparent at top to opaque at bottom
            // This creates the effect where the blue triangle blends with the red underneath
            let mut vtx_a2 = Vertex::new(config.scale_x(0.5), config.scale_y(0.9));
            let mut vtx_b2 = Vertex::new(config.scale_x(0.8), config.scale_y(0.1));
            let mut vtx_c2 = Vertex::new(config.scale_x(0.2), config.scale_y(0.1));

            // Top vertex (A) is fully opaque, bottom vertices fade to transparent
            // This creates the fade effect towards the bottom
            vtx_a2.a = 255.0; // Fully opaque at bottom
            vtx_b2.a = 0.0; // Transparent at top-right
            vtx_c2.a = 0.0; // Transparent at top-left

            // Blue: ABGR = A:FF, B:FF, G:00, R:00
            grConstantColorValue(Color(0xFFFF0000));
            grDrawTriangle(&vtx_a2, &vtx_b2, &vtx_c2);

            grBufferSwap(1);
        },
    );
}
