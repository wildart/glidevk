//! test05 - Two interpenetrating triangles with z-buffering
//!
//! This is a Rust port of the original Glide test05.c
//!
//! Usage: test05 [-n <frames>] [-r <resolution>]

use glide3x::testutil;
use glide3x::{
    grBufferClear, grBufferSwap, grConstantColorValue, grDepthBufferFunction, grDepthBufferMode,
    grDepthMask, grDrawTriangle, Alpha, CmpFnc, Color, DepthBufferMode, FxBool, Vertex,
};

fn main() {
    testutil::run_test(
        "test05 - Z-buffering",
        |_config| {
            // Set up z-buffering
            grDepthBufferMode(DepthBufferMode::ZBuffer);
            grDepthBufferFunction(CmpFnc::Greater);
            grDepthMask(FxBool::True);
        },
        |config| {
            grBufferClear(Color(0x00000000), Alpha(0), 0xFFFF);

            // First triangle (gray, closer z=10)
            let z_dist = 10.0f32;
            let ooz = 65535.0 / z_dist;

            let mut vtx_a = Vertex::new(config.scale_x(0.25), config.scale_y(0.21));
            vtx_a.ooz = ooz;
            let mut vtx_b = Vertex::new(config.scale_x(0.75), config.scale_y(0.21));
            vtx_b.ooz = ooz;
            let mut vtx_c = Vertex::new(config.scale_x(0.5), config.scale_y(0.79));
            vtx_c.ooz = ooz;

            // Gray color with full alpha: ABGR = A:FF, B:80, G:80, R:80
            grConstantColorValue(Color(0xFF808080));
            grDrawTriangle(&vtx_a, &vtx_b, &vtx_c);

            // Second triangle (green, varying z - 12.5 at top, 7.5 at bottom)
            let z_dist_far = 12.5f32;
            let z_dist_near = 7.5f32;

            let mut vtx_a2 = Vertex::new(config.scale_x(0.86), config.scale_y(0.21));
            vtx_a2.ooz = 65535.0 / z_dist_far;
            let mut vtx_b2 = Vertex::new(config.scale_x(0.86), config.scale_y(0.79));
            vtx_b2.ooz = 65535.0 / z_dist_far;
            let mut vtx_c2 = Vertex::new(config.scale_x(0.14), config.scale_y(0.5));
            vtx_c2.ooz = 65535.0 / z_dist_near;

            // Green color with full alpha: ABGR = A:FF, B:00, G:FF, R:00
            grConstantColorValue(Color(0xFF00FF00));
            grDrawTriangle(&vtx_a2, &vtx_b2, &vtx_c2);

            grBufferSwap(1);
        },
    );
}
