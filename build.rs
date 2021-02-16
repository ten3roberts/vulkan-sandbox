use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;

use std::fs;
use std::process::Command;

// Compiles all shaders recursively
fn compile_shaders() -> Result<()> {
    let shader_path = "./data/shaders";
    println!("cargo:rerun-if-changed={}", shader_path);
    for entry in fs::read_dir(shader_path)? {
        let entry = entry?;
        let path = entry.path();
        let path = path.to_str().expect("Invalid UTF-8 path");

        // Not a shader
        if !path.ends_with(".vert") && !path.ends_with(".frag") {
            continue;
        }

        let out = format!("{}.spv", path);

        // Tell cargo to rerun if this file changes
        println!("cargo:rerun-if-changed={}", path);

        eprintln!("Compiling shader: '{}' -> '{}'", path, out);

        let status = Command::new("glslc")
            .args(&[path, "-o", &out])
            .status()
            .context("glslc command not found")?;

        if !status.success() {
            return Err(anyhow!("Failed to compile shader"));
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    compile_shaders()?;
    Ok(())
}
