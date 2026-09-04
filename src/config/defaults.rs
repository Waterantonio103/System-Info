use ratatui::style::Color;
use serde::{Deserialize, Serialize};

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
    pub config: char,
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
            config: 'z',
        }
    }
}

impl Keybinds {
    pub fn set(&mut self, index: usize, key: char) -> Result<(), String> {
        if index >= self.len() {
            return Err(format!("invalid keybind index: {index}"));
        }

        if self
            .iter()
            .enumerate()
            .any(|(other_index, (_, other_key))| other_index != index && other_key == key)
        {
            return Err(format!("'{key}' is already in use"));
        }

        match index {
            0 => self.quit = key,
            1 => self.system = key,
            2 => self.processor = key,
            3 => self.graphics = key,
            4 => self.disk = key,
            5 => self.processes = key,
            6 => self.memory = key,
            7 => self.network = key,
            8 => self.config = key,
            _ => unreachable!(),
        }

        Ok(())
    }

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
            ("config", self.config),
        ];

        let mut errors: Vec<(char, Vec<String>)> = Vec::new();
        for (index, (name, key)) in bindings.iter().enumerate() {
            let mut names: Vec<String> = Vec::new();
            if errors.iter().any(|(k, _)| k == key) {
                continue;
            }

            for (other_name, other_key) in &bindings[index + 1..] {
                if key == other_key {
                    names.push((*name).to_string());
                    names.push((*other_name).to_string());
                }
            }
            if !names.is_empty() {
                names.sort();
                names.dedup();
                errors.push((*key, names));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, char)> {
        [
            ("quit", self.quit),
            ("system", self.system),
            ("processor", self.processor),
            ("graphics", self.graphics),
            ("disk", self.disk),
            ("processes", self.processes),
            ("memory", self.memory),
            ("network", self.network),
            ("config", self.config),
        ]
        .into_iter()
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }
}

#[cfg(test)]
mod keybind_tests {
    use super::Keybinds;

    #[test]
    fn sets_the_selected_keybind() {
        let mut keybinds = Keybinds::default();

        assert_eq!(keybinds.set(1, 'x'), Ok(()));
        assert_eq!(keybinds.system, 'x');
    }

    #[test]
    fn rejects_a_keybind_that_is_already_in_use() {
        let mut keybinds = Keybinds::default();

        assert_eq!(
            keybinds.set(1, keybinds.quit),
            Err("'q' is already in use".to_string())
        );
        assert_eq!(keybinds.system, 's');
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

impl Colors {
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, ConfigColor)> {
        [
            ("disk", self.disk),
            ("processes", self.processes),
            ("memory", self.memory),
            ("network", self.network),
        ]
        .into_iter()
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigColor {
    #[default]
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

impl ConfigColor {
    pub fn iter() -> impl Iterator<Item = Self> {
        [
            Self::Reset,
            Self::Black,
            Self::Red,
            Self::Green,
            Self::Yellow,
            Self::Blue,
            Self::Magenta,
            Self::Cyan,
            Self::Gray,
            Self::DarkGray,
            Self::LightRed,
            Self::LightGreen,
            Self::LightYellow,
            Self::LightBlue,
            Self::LightMagenta,
            Self::LightCyan,
            Self::White,
            Self::Rgb(0, 0, 0),
            Self::Indexed(0),
        ]
        .into_iter()
    }

    pub fn picker_label(self) -> &'static str {
        match self {
            Self::Reset => "Reset",
            Self::Black => "Black",
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Yellow => "Yellow",
            Self::Blue => "Blue",
            Self::Magenta => "Magenta",
            Self::Cyan => "Cyan",
            Self::Gray => "Gray",
            Self::DarkGray => "Dark Gray",
            Self::LightRed => "Light Red",
            Self::LightGreen => "Light Green",
            Self::LightYellow => "Light Yellow",
            Self::LightBlue => "Light Blue",
            Self::LightMagenta => "Light Magenta",
            Self::LightCyan => "Light Cyan",
            Self::White => "White",
            Self::Rgb(_, _, _) => "RGB...",
            Self::Indexed(_) => "Indexed...",
        }
    }

    pub fn len() -> usize {
        ConfigColor::iter().count()
    }

    pub fn select(index: usize) -> Self {
        ConfigColor::iter().nth(index).unwrap_or(ConfigColor::White)
    }
}

#[cfg(test)]
mod color_tests {
    use super::{Colors, ConfigColor};

    #[test]
    fn custom_colors_survive_a_toml_round_trip() {
        let colors = Colors {
            disk: ConfigColor::Rgb(12, 34, 56),
            network: ConfigColor::Indexed(201),
            ..Colors::default()
        };

        let encoded = toml::to_string_pretty(&colors).unwrap();
        let decoded: Colors = toml::from_str(&encoded).unwrap();

        assert!(matches!(decoded.disk, ConfigColor::Rgb(12, 34, 56)));
        assert!(matches!(decoded.network, ConfigColor::Indexed(201)));
    }
}
