use std::{env, fs, path::PathBuf};

#[path = "../../tools/gpui_build.rs"]
mod gpui_build;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let upstream = gpui_build::component_source(&manifest)?;
    let generated = output.join("file-menu");
    fs::create_dir_all(&generated)?;
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    for name in [
        "context_menu.rs",
        "popup_menu.rs",
        "menu_item.rs",
        "dropdown_menu.rs",
    ] {
        let source = upstream.join("src/menu").join(name);
        println!("cargo:rerun-if-changed={}", source.display());
        let source = fs::read_to_string(source)?;
        let source = source.split("\n#[cfg(test)]").next().unwrap();
        let source = source
            .replace("use crate::", "use gpui_kit::component::")
            .replace("gpui_kit::component::actions::", "gpui_kit::base::actions::")
            .replace("gpui_base::", "gpui_kit::base::")
            .replace("use gpui_kit::component::menu::menu_item::MenuItemElement;", "use super::menu_item::MenuItemElement;")
            .replace("use gpui_kit::component::{Selectable, button::Button, menu::PopupMenu, popover::Popover};", "use gpui_kit::component::{Selectable, button::Button, popover::Popover};\nuse super::PopupMenu;")
            .replace("use gpui_kit::component::{input::AnyInputState, menu::PopupMenu, root::WindowState};", "use gpui_kit::component::{input::AnyInputState, WindowExt as _};\nuse super::{MenuInputExt as _, PopupMenu};")
            .replace("WindowState::try_update(window, cx, |state, _, _| state.focused_input.clone())\n                    .flatten()", "window.focused_input(cx)")
            .replace("input.input_bounds(cx)", "input.menu_input_bounds(cx)")
            .replace("input.set_selection_focus(", "input.set_menu_selection_focus(");
        if source.contains("WindowState::") {
            return Err(
                "The pinned context-menu source changed its focused-input integration.".into(),
            );
        }
        fs::write(generated.join(name), source)?;
    }
    gpui_build::apply_patch(
        &generated,
        &manifest.join("src/app/widgets/menu-style.patch"),
    )?;
    fs::write(
        output.join("file-menu-modules.rs"),
        concat!(
            "#[path = \"file-menu/context_menu.rs\"]\nmod context_menu;\n",
            "#[path = \"file-menu/popup_menu.rs\"]\nmod popup_menu;\n",
            "#[path = \"file-menu/menu_item.rs\"]\nmod menu_item;\n",
            "#[path = \"file-menu/dropdown_menu.rs\"]\nmod dropdown_menu;\n",
        ),
    )?;
    Ok(())
}
