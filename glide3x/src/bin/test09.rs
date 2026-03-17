//! test09 - Chromakey test
//!
//! This is a Rust port of the original Glide test09.c
//! Renders red and blue triangles, chromakeys one out.
//! Uses grSstOrigin to flip the origin for the second triangle.
//!
//! Usage: test09 [-n <frames>] [-r <resolution>]
//!   Press SPACE to toggle chromakey color between RED and BLUE
//!   Press any other key to exit

use glide3x::testutil;
use glide3x::{
    grBufferClear, grBufferSwap, grChromakeyMode, grChromakeyValue, grColorCombine,
    grConstantColorValue, grDrawTriangle, grSstOrigin, grVertexLayout, Alpha, ChromakeyMode, Color,
    CombineFactor, CombineFunction, CombineLocal, CombineOther, FxBool, OriginLocation, Vertex,
};

// Vertex layout parameters
const GR_PARAM_XY: u32 = 0x01;
const GR_PARAM_ENABLE: u32 = 0x01;

const RED: u32 = 0x000000FF; // ABGR: A=00, B=00, G=00, R=FF
const BLUE: u32 = 0x00FF0000; // ABGR: A=00, B=FF, G=00, R=00

fn main() {
    use std::cell::Cell;
    use std::rc::Rc;

    // Track current chromakey color (can be toggled with SPACE)
    let chroma_color = Rc::new(Cell::new(BLUE));
    let chroma_color_render = Rc::clone(&chroma_color);

    testutil::run_test_interactive(
        "test09 - Chromakey (SPACE toggles color, other key exits)",
        move |_config| {
            // Set up vertex layout
            grVertexLayout(GR_PARAM_XY, 0, GR_PARAM_ENABLE);

            // Set up render state - constant color
            grColorCombine(
                CombineFunction::Local,
                CombineFactor::Zero,
                CombineLocal::Constant,
                CombineOther::Constant,
                FxBool::False,
            );

            // Enable chromakey on blue
            grChromakeyMode(ChromakeyMode::Enable);
            grChromakeyValue(Color(chroma_color.get()));

            println!("Press SPACE to toggle chromakey color");
            println!("Press any other key to quit");
            println!("Chromakey color: BLUE");
        },
        move |config, keys| {
            // Handle key input
            for key in keys {
                if *key == ' ' {
                    // Toggle chromakey color
                    let new_color = if chroma_color_render.get() == BLUE {
                        RED
                    } else {
                        BLUE
                    };
                    chroma_color_render.set(new_color);
                    grChromakeyValue(Color(new_color));

                    let color_name = if new_color == RED { "RED" } else { "BLUE" };
                    println!("Chromakey color: {}", color_name);
                } else {
                    // Any other key exits
                    return false;
                }
            }

            grBufferClear(Color(0x00000000), Alpha(0), 0);

            // Triangle vertices (same for both, origin flip creates mirror)
            let vtx_a = Vertex::new(config.scale_x(0.5), config.scale_y(0.1));
            let vtx_b = Vertex::new(config.scale_x(0.8), config.scale_y(0.9));
            let vtx_c = Vertex::new(config.scale_x(0.2), config.scale_y(0.9));

            // Draw red triangle with upper-left origin
            grConstantColorValue(Color(RED));
            grDrawTriangle(&vtx_a, &vtx_b, &vtx_c);

            // Flip origin to lower-left
            grSstOrigin(OriginLocation::LowerLeft);

            // Draw blue triangle (will be mirrored due to origin flip)
            grConstantColorValue(Color(BLUE));
            grDrawTriangle(&vtx_a, &vtx_b, &vtx_c);

            // Reset origin to upper-left
            grSstOrigin(OriginLocation::UpperLeft);

            grBufferSwap(1);

            true // Continue running
        },
    );
}
