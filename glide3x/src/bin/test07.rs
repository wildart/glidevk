//! test07 - Alpha blending test
//!
//! This is a Rust port of the original Glide test07.c
//! Renders two triangles with alpha blending
//!
//! Usage: test07 [-n <frames>] [-r <resolution>]

use glide3x::testutil;
use glide3x::{
    grAlphaBlendFunction, grAlphaCombine, grBufferClear, grBufferSwap, grColorCombine,
    grConstantColorValue, grDrawTriangle, grVertexLayout, Alpha, AlphaBlendFnc, Color,
    CombineFactor, CombineFunction, CombineLocal, CombineOther, FxBool, Vertex,
};

// Vertex layout parameters (from glide.h)
const GR_PARAM_XY: u32 = 0x01;
const GR_PARAM_ENABLE: u32 = 0x01;

fn main() {
    testutil::run_test(
        "test07 - Alpha blending",
        |_config| {
            // Set up vertex layout (XY coordinates enabled)
            grVertexLayout(GR_PARAM_XY, 0, GR_PARAM_ENABLE);

            // Set up render state - flat shading + alpha blend on constant alpha
            grColorCombine(
                CombineFunction::Local,
                CombineFactor::Zero,
                CombineLocal::Constant,
                CombineOther::Constant,
                FxBool::False,
            );

            grAlphaCombine(
                CombineFunction::Local,
                CombineFactor::Zero,
                CombineLocal::Constant,
                CombineOther::Constant,
                FxBool::False,
            );

            // Set up alpha blending: src_alpha, one_minus_src_alpha
            grAlphaBlendFunction(
                AlphaBlendFnc::SrcAlpha,
                AlphaBlendFnc::OneMinusSrcAlpha,
                AlphaBlendFnc::Zero,
                AlphaBlendFnc::Zero,
            );
        },
        |config| {
            grBufferClear(Color(0x00000000), Alpha(0), 0);

            // First triangle (red, fully opaque alpha=0xFF)
            let vtx_a = Vertex::new(config.scale_x(0.25), config.scale_y(0.21));
            let vtx_b = Vertex::new(config.scale_x(0.75), config.scale_y(0.21));
            let vtx_c = Vertex::new(config.scale_x(0.5), config.scale_y(0.79));

            // 0xFF0000FF = Alpha=FF, Blue=00, Green=00, Red=FF (red, opaque)
            grConstantColorValue(Color(0xFF0000FF));
            grDrawTriangle(&vtx_a, &vtx_b, &vtx_c);

            // Second triangle (blue, 50% transparent alpha=0x80)
            let vtx_a2 = Vertex::new(config.scale_x(0.86), config.scale_y(0.21));
            let vtx_b2 = Vertex::new(config.scale_x(0.86), config.scale_y(0.79));
            let vtx_c2 = Vertex::new(config.scale_x(0.14), config.scale_y(0.5));

            // 0x80FF0000 = Alpha=80, Blue=FF, Green=00, Red=00 (blue, 50% alpha)
            grConstantColorValue(Color(0x80FF0000));
            grDrawTriangle(&vtx_a2, &vtx_b2, &vtx_c2);

            grBufferSwap(1);
        },
    );
}
