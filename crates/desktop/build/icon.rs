use std::{error::Error, fs, path::Path};

use resvg::{tiny_skia, usvg};

#[cfg(windows)]
const WINDOWS_ICON_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

#[cfg(target_os = "macos")]
const MACOS_ICON_ENTRIES: [([u8; 4], u32); 11] = [
    (*b"icp4", 16),
    (*b"icp5", 32),
    (*b"ic11", 32),
    (*b"icp6", 64),
    (*b"ic12", 64),
    (*b"ic07", 128),
    (*b"ic13", 256),
    (*b"ic08", 256),
    (*b"ic14", 512),
    (*b"ic09", 512),
    (*b"ic10", 1024),
];

pub(super) fn compile(manifest: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let source = manifest.join("assets/ritkraken.svg");
    let options = usvg::Options {
        resources_dir: source.parent().map(Path::to_path_buf),
        ..Default::default()
    };
    let tree = usvg::Tree::from_data(&fs::read(&source)?, &options)?;
    compile_platform(manifest, output, &tree)
}

#[cfg(windows)]
fn compile_platform(
    manifest: &Path,
    output: &Path,
    tree: &usvg::Tree,
) -> Result<(), Box<dyn Error>> {
    use image::{
        ExtendedColorType,
        codecs::ico::{IcoEncoder, IcoFrame},
    };

    let frames = WINDOWS_ICON_SIZES
        .into_iter()
        .map(|size| {
            let png = render_png(tree, size)?;
            Ok(IcoFrame::with_encoded(
                png,
                size,
                size,
                ExtendedColorType::Rgba8,
            )?)
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    IcoEncoder::new(fs::File::create(output.join("ritkraken.ico"))?).encode_images(&frames)?;

    println!("cargo:rerun-if-changed=assets/windows.rc");
    embed_resource::compile(
        manifest.join("assets/windows.rc"),
        embed_resource::ParamsIncludeDirs([output]),
    )
    .manifest_required()?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn compile_platform(_: &Path, output: &Path, tree: &usvg::Tree) -> Result<(), Box<dyn Error>> {
    use std::collections::HashMap;

    let mut rendered = HashMap::new();
    let mut chunks = Vec::new();
    for (tag, size) in MACOS_ICON_ENTRIES {
        let png = match rendered.get(&size) {
            Some(existing) => existing,
            None => {
                let encoded = render_png(tree, size)?;
                rendered.entry(size).or_insert(encoded)
            }
        };
        let chunk_len = u32::try_from(8 + png.len())?;
        chunks.extend_from_slice(&tag);
        chunks.extend_from_slice(&chunk_len.to_be_bytes());
        chunks.extend_from_slice(png);
    }
    let total_len = u32::try_from(8 + chunks.len())?;
    let mut icns = Vec::with_capacity(total_len as usize);
    icns.extend_from_slice(b"icns");
    icns.extend_from_slice(&total_len.to_be_bytes());
    icns.extend_from_slice(&chunks);
    fs::write(output.join("RitKraken.icns"), icns)?;
    Ok(())
}

fn render_png(tree: &usvg::Tree, size: u32) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut pixmap = tiny_skia::Pixmap::new(size, size).ok_or("Could not allocate the app icon")?;
    let side = size as f32;
    let bounds = tree.size();
    let scale = side / bounds.width().max(bounds.height());
    let transform = tiny_skia::Transform::from_row(
        scale,
        0.0,
        0.0,
        scale,
        (side - bounds.width() * scale) / 2.0,
        (side - bounds.height() * scale) / 2.0,
    );
    resvg::render(tree, transform, &mut pixmap.as_mut());
    Ok(pixmap.encode_png()?)
}
