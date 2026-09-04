pub mod defaults;

use color_eyre::eyre::{self, eyre};
pub use defaults::{Colors, ConfigColor, Keybinds};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub keybinds: Keybinds,
    pub colors: Colors,
}

pub fn load_config() -> eyre::Result<Config> {
    let config_path = config_path()?;

    if !config_path.exists() {
        return write_cfg(&config_path);
    }

    let contents = fs::read_to_string(&config_path)?;

    let config = if contents.trim().is_empty() {
        write_cfg(&config_path)?
    } else {
        match toml::from_str::<Config>(&contents) {
            Ok(cfg) => {
                let updated = toml::to_string_pretty(&cfg)?;

                if updated != contents {
                    fs::write(&config_path, updated)?;
                }

                cfg
            }
            Err(_) => write_cfg(&config_path)?,
        }
    };

    if let Err(errors) = config.keybinds.validate() {
        return Err(eyre!(
            "invalid keybind configuration:\n- {}",
            errors
                .iter()
                .map(|(key, bindings)| {
                    let dupes = bindings.len();
                    let conflicts = bindings.join(", ");
                    format!("Key '{key}' duplicated {dupes} times: {conflicts}")
                })
                .collect::<Vec<String>>()
                .join("\n- ")
        ));
    }

    Ok(config)
}

fn config_path() -> eyre::Result<PathBuf> {
    let project_dirs = ProjectDirs::from("com", "waterantonio103", "system-watch")
        .ok_or_else(|| eyre!("Could not determine config directory"))?;

    let config_dir = project_dirs.config_dir();
    let config_path = config_dir.join("syswatch.toml");

    fs::create_dir_all(config_dir)?;

    Ok(config_path)
}

fn write_cfg(path: &Path) -> eyre::Result<Config> {
    let config = Config::default();
    let contents = toml::to_string_pretty(&config)?;

    fs::write(path, contents)?;

    Ok(config)
}

pub fn save_cfg(config: &Config) -> eyre::Result<()> {
    let config_path = config_path()?;
    let contents = toml::to_string_pretty(config)?;

    fs::write(config_path, contents)?;

    Ok(())
}
