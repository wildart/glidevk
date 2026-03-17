//! test14 - Depth buffer test
//!
//! This is a Rust port of the original Glide test14.c
//! The original tested depth bias with interpenetrating triangles.
//! This version demonstrates basic depth buffering with overlapping triangles.
//!
//! Note: grDepthBiasLevel is not implemented, so this is a simplified version
//! that demonstrates depth buffer functionality.
//!
//! Usage: test14 [-n <frames>] [-r <resolution>] [-d <filename>]

use glide3x::testutil;
use glide3x::{
    grBufferClear, grBufferSwap, grConstantColorValue, grDepthBufferFunction, grDepthBufferMode,
    grDepthMask, grDrawTriangle, Alpha, CmpFnc, Color, DepthBufferMode, Vertex,
};

fn main() {
    testutil::run_test(
        "test14 - Depth Buffer Test",
        |_config| {
            // Enable z-buffering
            grDepthBufferMode(DepthBufferMode::ZBuffer);
            grDepthBufferFunction(CmpFnc::Greater); // Greater Z wins (Glide convention)
            grDepthMask(true.into());
            println!("Depth buffer test - two interpenetrating triangles");
        },
        |config| {
            // Clear with max depth (0xFFFF for 16-bit depth buffer)
            grBufferClear(Color(0xFF000000), Alpha(0), 0xFFFF);

            // Gray triangle - horizontal, at medium depth
            // Vertices at different Z values to create an angled surface
            let mut vtx_a = Vertex::new(config.scale_x(0.25), config.scale_y(0.21));
            let mut vtx_b = Vertex::new(config.scale_x(0.75), config.scale_y(0.21));
            let mut vtx_c = Vertex::new(config.scale_x(0.5), config.scale_y(0.79));

            // Set Z values (ooz = 1/z, so larger ooz = closer)
            let z_dist = 10.0;
            vtx_a.ooz = 65535.0 / z_dist;
            vtx_b.ooz = 65535.0 / z_dist;
            vtx_c.ooz = 65535.0 / z_dist;

            // Gray: ABGR = A:FF, B:80, G:80, R:80
            grConstantColorValue(Color(0xFF808080));
            grDrawTriangle(&vtx_a, &vtx_b, &vtx_c);

            // Green triangle - vertical, interpenetrates with gray
            // Front vertex is closer, back vertices are farther
            let mut vtx_a2 = Vertex::new(config.scale_x(0.86), config.scale_y(0.21));
            let mut vtx_b2 = Vertex::new(config.scale_x(0.86), config.scale_y(0.79));
            let mut vtx_c2 = Vertex::new(config.scale_x(0.14), config.scale_y(0.5));

            // Different Z values - C is closer (higher ooz), A and B are farther
            let z_far = 12.5;
            let z_near = 7.5;
            vtx_a2.ooz = 65535.0 / z_far;
            vtx_b2.ooz = 65535.0 / z_far;
            vtx_c2.ooz = 65535.0 / z_near; // Closer to camera

            // Green: ABGR = A:FF, B:00, G:FF, R:00
            grConstantColorValue(Color(0xFF00FF00));
            grDrawTriangle(&vtx_a2, &vtx_b2, &vtx_c2);

            grBufferSwap(1);
        },
    );
}
