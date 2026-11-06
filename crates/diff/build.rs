use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

const UPSTREAM_VERSION: &str = "0.9.0";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed=../../Cargo.lock");

    let upstream = match cached_source() {
        Some(source) => source,
        None => upstream_source(&manifest_dir)?,
    };
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
    ] {
        let patch = manifest_dir.join(name);
        println!("cargo:rerun-if-changed={}", patch.display());
        let result = Command::new("git")
            .current_dir(&generated)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_CEILING_DIRECTORIES", &output_dir)
            .args([
                "-c",
                "core.autocrlf=false",
                "apply",
                "--no-index",
                "--whitespace=nowarn",
            ])
            .arg(patch)
            .output()?;
        if !result.status.success() {
            return Err(format!(
                "Could not apply {name}: {}",
                String::from_utf8_lossy(&result.stderr)
            )
            .into());
        }
    }
    fs::write(
        output_dir.join("modules.rs"),
        "#[path = \"gpui-component/diff/mod.rs\"]\npub mod diff;\n\
         #[path = \"gpui-component/highlighter/language_name.rs\"]\nmod language_name;\n\
         #[allow(dead_code)]\n#[path = \"gpui-component/highlighter/syntect_backend.rs\"]\nmod syntect_backend;\n",
    )?;
    Ok(())
}

fn cached_source() -> Option<PathBuf> {
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::home_dir().map(|home| home.join(".cargo")))?;
    let package = format!("gpui-component-{UPSTREAM_VERSION}");
    let mut sources = fs::read_dir(cargo_home.join("registry/src"))
        .ok()?
        .filter_map(Result::ok)
        .map(|registry| registry.path().join(&package))
        .filter(|source| source.join(".cargo-ok").is_file());
    let source = sources.next()?;
    sources.next().is_none().then_some(source)
}

fn upstream_source(manifest_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let output = Command::new(env::var_os("CARGO").unwrap())
        .current_dir(manifest_dir)
        .args(["metadata", "--format-version=1", "--locked", "--offline"])
        .arg("--filter-platform")
        .arg(env::var_os("TARGET").unwrap())
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "Could not locate the pinned GPUI sources: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let package = metadata["packages"].as_array().and_then(|packages| {
        packages.iter().find(|package| {
            package["name"] == "gpui-component" && package["version"] == UPSTREAM_VERSION
        })
    });
    package
        .and_then(|package| package["manifest_path"].as_str())
        .and_then(|manifest| Path::new(manifest).parent())
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("The pinned gpui-component source is missing").into())
}
