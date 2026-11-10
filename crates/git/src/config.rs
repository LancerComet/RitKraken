use std::{env, path::PathBuf};

use crate::{
    Result,
    command::{command, run_read},
    protocol::{path_from_bytes, without_final_newline},
};

pub fn global_config_path() -> Result<Option<PathBuf>> {
    let candidates = if let Some(path) = env::var_os("GIT_CONFIG_GLOBAL") {
        vec![PathBuf::from(path)]
    } else {
        // Expand the home path using the installed Git, without parsing any
        // existing config. This also works on Git versions without `git var GIT_CONFIG_GLOBAL`.
        let output = run_read(command(None).args([
            "config",
            "--file",
            "-",
            "--type=path",
            "--default",
            "~/.gitconfig",
            "--get",
            "ritkraken.globalConfigPath",
        ]))?;
        let home_config = path_from_bytes(without_final_newline(&output));
        let xdg_home = env::var_os("XDG_CONFIG_HOME")
            .filter(|path| !path.is_empty())
            .map_or_else(|| home_config.with_file_name(".config"), PathBuf::from);
        vec![home_config, xdg_home.join("git").join("config")]
    };
    Ok(candidates
        .into_iter()
        .find(|path| path.is_file())
        .map(std::path::absolute)
        .transpose()?)
}
