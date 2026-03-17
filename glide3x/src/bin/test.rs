//! test - Multiple triangles demo
//!
//! Renders multiple colored triangles including an animated one
//!
//! Press any key to exit

use glide3x::testutil;
use glide3x::{
    grBufferClear, grBufferSwap, grConstantColorValue, grDrawTriangle, Alpha, Color, Vertex,
};

fn main() {
    // Use a static frame counter for animation
    static mut FRAME_COUNT: u32 = 0;

    testutil::run_test_interactive(
        "GlideVK Test - Triangle Rendering (press any key to exit)",
        |_config| {
            println!("Press any key to exit");
        },
        |config, keys| {
            // Clear to dark blue (ABGR: A=FF, B=8C, G=00, R=00)
            grBufferClear(Color(0xFF8C0000), Alpha(0xff), 0);

            // Draw a large red triangle in the center
            // ABGR format: Red = 0xFF0000FF (A=FF, B=00, G=00, R=FF)
            grConstantColorValue(Color(0xFF0000FF)); // Red
            let v0 = Vertex::new(config.scale_x(0.5), config.scale_y(0.1));
            let v1 = Vertex::new(config.scale_x(0.2), config.scale_y(0.9));
            let v2 = Vertex::new(config.scale_x(0.8), config.scale_y(0.9));
            grDrawTriangle(&v0, &v1, &v2);

            // Draw a smaller yellow triangle on the left
            // ABGR format: Yellow = 0xFF00FFFF (A=FF, B=00, G=FF, R=FF)
            grConstantColorValue(Color(0xFF00FFFF)); // Yellow
            let v3 = Vertex::new(config.scale_x(0.15), config.scale_y(0.2));
            let v4 = Vertex::new(config.scale_x(0.05), config.scale_y(0.5));
            let v5 = Vertex::new(config.scale_x(0.25), config.scale_y(0.5));
            grDrawTriangle(&v3, &v4, &v5);

            // Draw a cyan triangle on the right
            // ABGR format: Cyan = 0xFFFFFF00 (A=FF, B=FF, G=FF, R=00)
            grConstantColorValue(Color(0xFFFFFF00)); // Cyan
            let v6 = Vertex::new(config.scale_x(0.85), config.scale_y(0.2));
            let v7 = Vertex::new(config.scale_x(0.75), config.scale_y(0.5));
            let v8 = Vertex::new(config.scale_x(0.95), config.scale_y(0.5));
            grDrawTriangle(&v6, &v7, &v8);

            // Draw a white triangle overlapping center
            grConstantColorValue(Color(0xFFFFFFFF)); // White
            let v9 = Vertex::new(config.scale_x(0.5), config.scale_y(0.4));
            let v10 = Vertex::new(config.scale_x(0.35), config.scale_y(0.7));
            let v11 = Vertex::new(config.scale_x(0.65), config.scale_y(0.7));
            grDrawTriangle(&v9, &v10, &v11);

            // Animated orange triangle at bottom
            // ABGR format: Orange = 0xFF00A5FF (A=FF, B=00, G=A5, R=FF)
            let frame_count = unsafe {
                FRAME_COUNT += 1;
                FRAME_COUNT
            };
            let offset = (frame_count as f32 * 0.05).sin() * 0.1;
            grConstantColorValue(Color(0xFF00A5FF)); // Orange
            let v12 = Vertex::new(config.scale_x(0.5 + offset), config.scale_y(0.75));
            let v13 = Vertex::new(config.scale_x(0.4 + offset), config.scale_y(0.95));
            let v14 = Vertex::new(config.scale_x(0.6 + offset), config.scale_y(0.95));
            grDrawTriangle(&v12, &v13, &v14);

            grBufferSwap(1);

            // Exit if any key pressed
            keys.is_empty()
        },
    );
}
