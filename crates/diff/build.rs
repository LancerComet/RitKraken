use std::{env, fs, path::PathBuf};

#[path = "../../tools/gpui_build.rs"]
mod gpui_build;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed=../../Cargo.lock");

    let upstream = gpui_build::component_source(&manifest_dir)?;
    let generated = output_dir.join("gpui-component");
    for module in ["diff", "highlighter"] {
        fs::create_dir_all(generated.join(module))?;
    }
    for file in [
        "diff/conflict.rs",
        "diff/decoration.rs",
        "diff/document.rs",
        "diff/mod.rs",
        "diff/parser.rs",
        "diff/presentation.rs",
        "diff/selection.rs",
        "diff/state.rs",
        "highlighter/language_name.rs",
        "highlighter/syntect_backend.rs",
    ] {
        let source = upstream.join("src").join(file);
        println!("cargo:rerun-if-changed={}", source.display());
        fs::copy(source, generated.join(file))?;
    }
    for name in [
        "split-viewport.patch",
        "change-controls.patch",
        "edge-whitespace.patch",
        "editor-typography.patch",
        "horizontal-scroll.patch",
        "overview.patch",
    ] {
        let patch = manifest_dir.join(name);
        gpui_build::apply_patch(&generated, &patch)?;
    }
    fs::write(
        output_dir.join("modules.rs"),
        "#[path = \"gpui-component/diff/mod.rs\"]\npub mod diff;\n\
         #[path = \"gpui-component/highlighter/language_name.rs\"]\nmod language_name;\n\
         #[allow(dead_code)]\n#[path = \"gpui-component/highlighter/syntect_backend.rs\"]\nmod syntect_backend;\n",
    )?;
    Ok(())
}
