use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

const UPSTREAM_VERSION: &str = "0.9.0";

pub fn component_source(manifest_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    match cached_source() {
        Some(source) => Ok(source),
        None => upstream_source(manifest_dir),
    }
}

pub fn apply_patch(directory: &Path, patch: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed={}", patch.display());
    let result = Command::new("git")
        .current_dir(directory)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env("GIT_CEILING_DIRECTORIES", directory.parent().unwrap())
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
            "Could not apply {}: {}",
            patch.display(),
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
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
