use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Export definition: (function_name, parameter_size_in_bytes)
/// Parameter size is the total size of all parameters pushed on the stack (for stdcall decoration)
const EXPORTS: &[(&str, u32)] = &[
    // Glide management
    ("grGlideInit", 0),
    ("grGlideShutdown", 0),
    // SST functions
    ("grSstSelect", 4),
    ("grSstWinOpen", 28), // 7 params * 4 bytes
    ("grSstWinClose", 4),
    ("grSstScreenWidth", 0),
    ("grSstScreenHeight", 0),
    ("grSstOrigin", 4),
    ("grSelectContext", 4),
    // Buffer functions
    ("grBufferClear", 12), // 3 params * 4 bytes
    ("grBufferSwap", 4),
    ("grClipWindow", 16), // 4 params * 4 bytes
    ("grRenderBuffer", 4),
    // State functions
    ("grDepthBufferMode", 4),
    ("grDepthBufferFunction", 4),
    ("grDepthMask", 4),
    ("grColorMask", 8),           // 2 params * 4 bytes
    ("grAlphaBlendFunction", 16), // 4 params * 4 bytes
    ("grCullMode", 4),
    ("grConstantColorValue", 4),
    ("grFogMode", 4),
    ("grFogColorValue", 4),
    ("grChromakeyMode", 4),
    ("grChromakeyValue", 4),
    ("grAlphaTestFunction", 4),
    ("grAlphaTestReferenceValue", 4),
    ("grEnable", 4),
    ("grDisable", 4),
    // Drawing functions
    ("grDrawTriangle", 12), // 3 pointers * 4 bytes
    ("grDrawPoint", 4),
    ("grDrawLine", 8),                   // 2 pointers * 4 bytes
    ("grDrawVertexArray", 12),           // mode + count + pointers = 3 * 4 bytes
    ("grDrawVertexArrayContiguous", 16), // mode + count + pointers + stride = 4 * 4 bytes
    // Texture functions
    ("grTexFilterMode", 12),
    ("grTexClampMode", 12),
    ("grTexMipMapMode", 12),
    ("grTexLodBiasValue", 8), // ChipID + f32
    ("grTexMinAddress", 4),
    ("grTexMaxAddress", 4),
    ("grTexCombine", 28),                    // 7 params * 4 bytes
    ("grTexDownloadMipMapLevel", 32),        // 8 params * 4 bytes
    ("grTexDownloadMipMapLevelPartial", 40), // 10 params * 4 bytes
    ("grTexDownloadTable", 8),               // type + data = 2 * 4 bytes
    ("grTexSource", 16),                     // 4 params * 4 bytes
    ("grTexDownloadMipMap", 16),
    ("grTexTextureMemRequired", 8),
    // Combine functions
    ("grColorCombine", 20), // 5 params * 4 bytes
    ("grAlphaCombine", 20),
    ("grVertexLayout", 12),
    ("grReset", 4),
    ("grCoordinateSpace", 4),
    ("grDitherMode", 4),
    ("grFinish", 0),
    // Query functions
    ("grGetString", 4),
    ("grGet", 12),
    ("grGetProcAddress", 4),
    // LFB functions
    ("grLfbLock", 24), // 6 params * 4 bytes
    ("grLfbUnlock", 8),
    ("grLfbReadRegion", 28), // 7 params * 4 bytes
    // Gamma functions
    ("grLoadGammaTable", 16),     // 4 params * 4 bytes
    ("guGammaCorrectionRGB", 12), // 3 floats * 4 bytes
];

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    if target_os == "windows" {
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let def_path = Path::new(&out_dir).join("glide3x.def");

        // Generate .def file
        let mut def_file = File::create(&def_path).expect("Failed to create .def file");

        writeln!(def_file, "LIBRARY glide3x").unwrap();
        writeln!(def_file, "EXPORTS").unwrap();
        writeln!(def_file, "    ; stdcall decorated aliases").unwrap();

        for (name, param_size) in EXPORTS {
            writeln!(def_file, "    _{}@{} = {}", name, param_size, name).unwrap();
        }

        def_file.flush().unwrap();

        // Pass .def file to linker
        if target_env == "gnu" {
            println!("cargo:rustc-cdylib-link-arg=-Wl,{}", def_path.display());
        } else if target_env == "msvc" {
            println!("cargo:rustc-cdylib-link-arg=/DEF:{}", def_path.display());
        }

        println!("cargo:rerun-if-changed=build.rs");
    }
}
