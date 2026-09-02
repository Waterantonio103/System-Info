mod defaults;

use defaults::*;
use std::{fs::{self, File}, io, path::PathBuf};
use serde::{Deserialize, Serialize};
use directories::ProjectDirs;
use color_eyre::{
    eyre::eyre,
    Result,
};
use ratatui::style::Color;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub keybinds: Keybinds,
    pub colors: Colors,
}

impl Default for Config {
    fn default() -> Self {
        Self { 
            keybinds: Keybinds::default(), 
            colors: Colors::default() 
        }
    }
}

pub fn load_config() -> Result<Config> {
    let project_dirs = ProjectDirs::from(
        "com",
        "waterantonio103",
        "system-watch",
    )
    .ok_or_else(|| eyre!("Could not determine config directory"))?;

    let config_dir = project_dirs.config_dir();
    let config_path = config_dir.join("syswatch.toml");

    dbg!(&config_dir);
    dbg!(&config_path);

    fs::create_dir_all(config_dir)?;

    if !config_path.exists() {
        let config = Config::default();
        let contents = toml::to_string_pretty(&config)?;

        fs::write(&config_path, contents)?;

        return Ok(config);
    }

    let contents = fs::read_to_string(&config_path)?;

    let config = if contents.trim().is_empty() {
        let config = Config::default();
        let contents = toml::to_string_pretty(&config)?;

        fs::write(&config_path, contents)?;

        config
    } else {
        toml::from_str(&contents)?
    };

    if let Err(errors) = config.keybinds.validate() {
        return Err(eyre!(
            "invalid keybind configuration:\n- {}",
            errors.iter()
                .map(|(key, bindings)| {
                    let dupes = bindings.len();
                    let mut conflicts = String::new();
                    for name in bindings.iter() {
                        let to_push = format!("{name}, ");
                        conflicts.push_str(&to_push);
                    }
                    format!("Key '{key}' duplicated {dupes} times: {conflicts}")
                })
                .collect::<Vec<String>>()
                .join("\n- ")
        ));
    }

    Ok(config)
}


