use color_eyre::eyre;
use serde::{Deserialize, Serialize};
use ratatui::style::Color;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Keybinds {
    pub quit: char,
    pub system: char,
    pub processor: char,
    pub graphics: char,
    pub disk: char,
    pub processes: char,
    pub memory: char,
    pub network: char,
}

impl Default for Keybinds {
    fn default() -> Self {
        Self {
            quit: 'q',
            system: 's',
            processor: 'c',
            graphics: 'g',
            disk: 'd',
            processes: 'p',
            memory: 'm',
            network: 'n',
        }
    }
}

impl Keybinds {
    pub fn validate(&self) -> Result<(), Vec<(char, Vec<String>)>> {
        let bindings = [
            ("quit", self.quit),
            ("system", self.system),
            ("processor", self.processor),
            ("graphics", self.graphics),
            ("disk", self.disk),
            ("processes", self.processes),
            ("memory", self.memory),
            ("network", self.network),
        ];

        
        let mut errors : Vec<(char, Vec<String>)> = Vec::new();
        for (index, (name, key)) in bindings.iter().enumerate() {
            let mut names: Vec<String> = Vec::new();
            if errors.iter().any(|(k,_)| k == key) {
                continue;
            } else {
                for (other_name, other_key) in &bindings[index + 1..] {
                    if key == other_key {
                        names.push(format!("{name}"));
                        names.push(format!("{other_name}"));
                    }
                }
                if !names.is_empty() {
                    names.sort();
                    names.dedup();
                    errors.push((*key, names));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    pub disk: ConfigColor,
    pub processes: ConfigColor,
    pub memory: ConfigColor,
    pub network: ConfigColor,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            disk: ConfigColor::Green,
            processes: ConfigColor::Blue,
            memory: ConfigColor::Red,
            network: ConfigColor::Cyan,
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum ConfigColor {
    Reset,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
    Rgb(u8, u8, u8),
    Indexed(u8),
}

impl From<ConfigColor> for Color {
    fn from(color: ConfigColor) -> Self {
        match color {
            ConfigColor::Reset => Color::Reset,
            ConfigColor::Black => Color::Black,
            ConfigColor::Red => Color::Red,
            ConfigColor::Green => Color::Green,
            ConfigColor::Yellow => Color::Yellow,
            ConfigColor::Blue => Color::Blue,
            ConfigColor::Magenta => Color::Magenta,
            ConfigColor::Cyan => Color::Cyan,
            ConfigColor::Gray => Color::Gray,
            ConfigColor::DarkGray => Color::DarkGray,
            ConfigColor::LightRed => Color::LightRed,
            ConfigColor::LightGreen => Color::LightGreen,
            ConfigColor::LightYellow => Color::LightYellow,
            ConfigColor::LightBlue => Color::LightBlue,
            ConfigColor::LightMagenta => Color::LightMagenta,
            ConfigColor::LightCyan => Color::LightCyan,
            ConfigColor::White => Color::White,
            ConfigColor::Rgb(red, green, blue) => Color::Rgb(red, green, blue),
            ConfigColor::Indexed(index) => Color::Indexed(index),
        }
    }
}
