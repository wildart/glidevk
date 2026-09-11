//! Textured quad with framebuffer assertions; run with -n 1.
use glide3x::*;

fn main() {
    testutil::run_test(
        "test16 - Texture upload and sampling",
        |_| {
            let pixels = [0xf800u16, 0x07e0, 0x001f, 0xffff];
            grTexDownloadMipMapLevel(
                ChipID::Tmu0, 0x1000, LOD::L2, LOD::L2, AspectRatio::A1x1,
                TextureFormat::Rgb565, 3, pixels.as_ptr().cast(),
            );
            let info = TexInfo::empty(LOD::L2, LOD::L2, AspectRatio::A1x1, TextureFormat::Rgb565);
            grTexSource(ChipID::Tmu0, 0x1000, 3, &info);
            grColorCombine(
                CombineFunction::ScaleOther, CombineFactor::One,
                CombineLocal::Iterated, CombineOther::Texture, FxBool::False,
            );
            grAlphaCombine(
                CombineFunction::ScaleOther, CombineFactor::One,
                CombineLocal::Iterated, CombineOther::Texture, FxBool::False,
            );
        },
        |config| {
            grBufferClear(Color(0), Alpha(0), 0xffff);
            let vertex = |x, y, s, t| {
                let mut v = Vertex::new(config.scale_x(x), config.scale_y(y));
                v.tmuvtx[0].sow = s;
                v.tmuvtx[0].tow = t;
                v
            };
            let a = vertex(0.25, 0.25, 0.0, 0.0);
            let b = vertex(0.75, 0.25, 256.0, 0.0);
            let c = vertex(0.75, 0.75, 256.0, 256.0);
            let d = vertex(0.25, 0.75, 0.0, 256.0);
            // Exercise a vertex upload spanning multiple memory pages.
            for _ in 0..125 {
                grDrawTriangle(&a, &b, &c);
                grDrawTriangle(&a, &c, &d);
            }
            grBufferSwap(1);
            for (x, y, expected) in [
                (0.375, 0.375, 0xf800u16),
                (0.625, 0.375, 0x07e0),
                (0.375, 0.625, 0x001f),
                (0.625, 0.625, 0xffff),
            ] {
                let mut pixel = 0u16;
                assert_eq!(grLfbReadRegion(
                    Buffer::Front, config.scale_x(x) as u32, config.scale_y(y) as u32,
                    1, 1, 2, (&mut pixel as *mut u16).cast(),
                ), FxBool::True);
                assert_eq!(pixel, expected, "Wrong texture color at ({x}, {y})");
            }
        },
    );
}
