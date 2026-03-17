//! test04 - Draws gouraud shaded triangle
//!
//! This is a Rust port of the original Glide test04.c
//! Draws a single triangle with red, green, and blue corners
//!
//! Usage: test04 [-n <frames>] [-r <resolution>]

use glide3x::testutil;
use glide3x::{
    grBufferClear, grBufferSwap, grColorCombine, grCoordinateSpace, grDrawTriangle, grVertexLayout,
    Alpha, Color, CombineFactor, CombineFunction, CombineLocal, CombineOther, CoordinateSpaceMode,
    FxBool, Vertex,
};

// Vertex layout parameters (from glide.h)
const GR_PARAM_XY: u32 = 0x01;
const GR_PARAM_RGB: u32 = 0x20;
const GR_PARAM_ENABLE: u32 = 0x01;
const GR_VERTEX_R_OFFSET: u32 = 4;

fn main() {
    testutil::run_test(
        "test04 - Gouraud shaded triangle",
        |_config| {
            // Set up vertex layout
            grVertexLayout(GR_PARAM_XY, 0, GR_PARAM_ENABLE);
            grVertexLayout(GR_PARAM_RGB, GR_VERTEX_R_OFFSET << 2, GR_PARAM_ENABLE);

            // Set up render state - gouraud shading (iterated vertex colors)
            grColorCombine(
                CombineFunction::Local,
                CombineFactor::Zero,
                CombineLocal::Iterated,
                CombineOther::Constant,
                FxBool::False,
            );

            // Set coordinate space to clip coordinates
            grCoordinateSpace(CoordinateSpaceMode::Clip);
        },
        |config| {
            grBufferClear(Color(0x00000000), Alpha(0), 0xFFFF);

            // Draw gouraud shaded triangle with RGB corners
            let mut vtx_a = Vertex::new(config.scale_x(0.3), config.scale_y(0.3));
            vtx_a.r = 255.0;
            vtx_a.g = 0.0;
            vtx_a.b = 0.0;
            vtx_a.a = 255.0;

            let mut vtx_b = Vertex::new(config.scale_x(0.8), config.scale_y(0.4));
            vtx_b.r = 0.0;
            vtx_b.g = 255.0;
            vtx_b.b = 0.0;
            vtx_b.a = 255.0;

            let mut vtx_c = Vertex::new(config.scale_x(0.5), config.scale_y(0.8));
            vtx_c.r = 0.0;
            vtx_c.g = 0.0;
            vtx_c.b = 255.0;
            vtx_c.a = 255.0;

            grDrawTriangle(&vtx_a, &vtx_b, &vtx_c);

            grBufferSwap(1);
        },
    );
}
