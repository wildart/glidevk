use std::{env, path::PathBuf, process::Command};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    println!("cargo:rerun-if-env-changed=GLSLC");
    for shader in ["glide.vert", "glide.frag"] {
        let source = format!("../shaders/{shader}");
        println!("cargo:rerun-if-changed={source}");
        let status = Command::new(env::var_os("GLSLC").unwrap_or_else(|| "glslc".into()))
            .arg(&source)
            .arg("-o")
            .arg(out.join(format!("{shader}.spv")))
            .status()
            .expect("Install glslc (Vulkan SDK/shaderc), or set GLSLC to its executable");
        assert!(status.success(), "Failed to compile {source}: {status}");
    }
}
