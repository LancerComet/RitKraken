use std::{error::Error, fs, path::Path};

use image::{
    ExtendedColorType,
    codecs::ico::{IcoEncoder, IcoFrame},
};
use resvg::{tiny_skia, usvg};

const ICON_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

pub(super) fn compile(manifest: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let source = manifest.join("assets/ritkraken.svg");
    let options = usvg::Options {
        resources_dir: source.parent().map(Path::to_path_buf),
        ..Default::default()
    };
    let tree = usvg::Tree::from_data(&fs::read(&source)?, &options)?;
    let frames = ICON_SIZES
        .into_iter()
        .map(|size| render_frame(&tree, size))
        .collect::<Result<Vec<_>, _>>()?;
    IcoEncoder::new(fs::File::create(output.join("ritkraken.ico"))?).encode_images(&frames)?;

    println!("cargo:rerun-if-changed=assets/windows.rc");
    embed_resource::compile(
        manifest.join("assets/windows.rc"),
        embed_resource::ParamsIncludeDirs([output]),
    )
    .manifest_required()?;
    Ok(())
}

fn render_frame(tree: &usvg::Tree, size: u32) -> Result<IcoFrame<'static>, Box<dyn Error>> {
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
    Ok(IcoFrame::with_encoded(
        pixmap.encode_png()?,
        size,
        size,
        ExtendedColorType::Rgba8,
    )?)
}
