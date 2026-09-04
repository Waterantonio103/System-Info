mod config;
mod models;

use config::{Config, ConfigColor, load_config, save_cfg};
use models::{
    CpuInfo, DeviceSelector, Disko, Gpu, Machine, Memory, NetworkInterface, Process, Processor,
    Swap,
};

use std::{
    cmp::Reverse,
    time::{Duration, Instant},
};

use all_smi::AllSmi;
use chrono::{DateTime, Local};
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{
        Alignment, Constraint,
        Direction::{Horizontal, Vertical},
        Layout, Rect,
    },
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        Axis, Bar, BarChart, Block, Cell, Chart, Dataset, Gauge, GraphType, List, ListItem,
        ListState, Paragraph, Row, Table, Wrap,
    },
};
use sysinfo::{DiskUsage, Disks, Networks, Pid, ProcessesToUpdate, System};

const DEFAULT_CPU_GROUP_SIZE: usize = 3;
const HISTORY_SAMPLES: usize = 60;

#[derive(Debug, Default)]
struct App {
    state: AppState,
    config: Config,
    config_dirty: bool,
    cfg_state: ConfigState,
    config_key_state: ListState,
    keybind_error: Option<String>,
    config_col_state: ListState,
    color_target: Option<ColorTarget>,
    config_edit_col_state: ListState,
    color_input: String,
    color_input_error: Option<String>,
    device: DeviceSelector,
    system: Machine,
    cpu: CpuInfo,
    cpus: Vec<Processor>,
    gpus: Vec<Gpu>,
    cpu_group_start: usize,
    cpu_group_size: usize,
    gpu_selection: usize,
    list_state: ListState,
    memory: Memory,
    processes: Vec<Process>,
    mem_offset: usize,
    process_selection: Option<Pid>,
    disks: Vec<Disko>,
    disk_selection: usize,
    networks: Vec<NetworkInterface>,
    network_selection: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum AppState {
    #[default]
    Running,
    Quitting,
    Config,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum ConfigState {
    #[default]
    None,
    Keybinds,
    KeybindInput,
    Colors,
    ColorPicker,
    ColorInput(ColorInputKind),
}

impl ConfigState {
    fn name(&self) -> &'static str {
        match self {
            Self::None => "Config",
            Self::Keybinds => "Config Keybinds",
            Self::KeybindInput => "Edit Keybind",
            Self::Colors => "Config Colors",
            Self::ColorPicker => "Pick Color",
            Self::ColorInput(ColorInputKind::Rgb) => "RGB Values",
            Self::ColorInput(ColorInputKind::Indexed) => "Indexed Value",
        }
    }

    fn keybind_description(&self, quit_key: char) -> String {
        match self {
            Self::None => {
                format!("Keybinds (k) | Colors (c) | Main (Esc) | Quit ({quit_key})")
            }
            Self::Keybinds => format!(
                "Navigate (Up/Down) | Edit (Enter) | Menu (k) | Main (Esc) | Quit ({quit_key})"
            ),
            Self::KeybindInput => String::from("Press a character | Cancel (Esc)"),
            Self::Colors => format!(
                "Navigate (Up/Down) | Edit (Enter) | Menu (c) | Main (Esc) | Quit ({quit_key})"
            ),
            Self::ColorPicker => format!(
                "Navigate (Up/Down) | Apply (Enter) | Colors (c) | Main (Esc) | Quit ({quit_key})"
            ),
            Self::ColorInput(ColorInputKind::Rgb) => {
                String::from("Enter R,G,B (0-255) | Apply (Enter) | Cancel (Esc)")
            }
            Self::ColorInput(ColorInputKind::Indexed) => {
                String::from("Enter index (0-255) | Apply (Enter) | Cancel (Esc)")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorInputKind {
    Rgb,
    Indexed,
}

#[derive(Debug, Clone, Copy)]
enum ColorTarget {
    Disk,
    Processes,
    Memory,
    Network,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let mut app = App {
        config: load_config()?,
        ..App::default()
    };

    ratatui::run(|terminal| app.run(terminal))
}

fn format_bytes(bytes: u64) -> (f64, &'static str) {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * KB;
    const GB: f64 = 1024.0 * MB;
    const TB: f64 = 1024.0 * GB;

    let bytes = bytes as f64;

    if bytes >= TB {
        (bytes / TB, "TB")
    } else if bytes >= GB {
        (bytes / GB, "GB")
    } else if bytes >= MB {
        (bytes / MB, "MB")
    } else if bytes >= KB {
        (bytes / KB, "KB")
    } else {
        (bytes, "B")
    }
}

fn percentage(used: u64, capacity: u64) -> u64 {
    if capacity == 0 {
        0
    } else {
        used.saturating_mul(100).saturating_div(capacity).min(100)
    }
}

fn indexed_title(label: &str, index: usize, total: usize) -> String {
    if total == 0 {
        label.to_string()
    } else {
        format!("{label} ({}/{total})", index + 1)
    }
}

fn parse_color_input(kind: ColorInputKind, input: &str) -> Result<ConfigColor, String> {
    let values = input
        .split(|character: char| character == ',' || character.is_whitespace())
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<u8>()
                .map_err(|_| String::from("values must be whole numbers from 0 to 255"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    match (kind, values.as_slice()) {
        (ColorInputKind::Rgb, [red, green, blue]) => Ok(ConfigColor::Rgb(*red, *green, *blue)),
        (ColorInputKind::Rgb, _) => Err(String::from("enter exactly three values: R,G,B")),
        (ColorInputKind::Indexed, [index]) => Ok(ConfigColor::Indexed(*index)),
        (ColorInputKind::Indexed, _) => Err(String::from("enter one palette index")),
    }
}

fn time_fmt(seconds: u64) -> String {
    const DAY_IN_SECS: u64 = 86400;
    const HOUR_IN_SECS: u64 = 3600;
    const MIN_IN_SECS: u64 = 60;

    let days = seconds / DAY_IN_SECS;
    let hours = seconds / HOUR_IN_SECS;
    let minutes = seconds / MIN_IN_SECS;

    if days > 0 {
        let remaining_hrs = seconds % DAY_IN_SECS;
        let hours = remaining_hrs / HOUR_IN_SECS;
        let remaining_mins = remaining_hrs % HOUR_IN_SECS;
        let mins = remaining_mins / MIN_IN_SECS;
        let secs = remaining_mins % MIN_IN_SECS;
        format!("{:02}d{:02}h{:02}m{:02}s", days, hours, mins, secs)
    } else if hours > 0 {
        let remaining_mins = seconds % HOUR_IN_SECS;
        let mins = remaining_mins / MIN_IN_SECS;
        let secs = remaining_mins % MIN_IN_SECS;
        format!("{:02}h{:02}m{:02}s", hours, mins, secs)
    } else {
        let secs = seconds % MIN_IN_SECS;
        format!("{:02}m{:02}s", minutes, secs)
    }
}

fn date_fmt(boot: u64) -> String {
    let boot_date = DateTime::from_timestamp(boot as i64, 0)
        .unwrap()
        .with_timezone(&Local);

    boot_date.format("%m/%d/%Y at %I:%M:%S %p").to_string()
}

impl App {
    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let mut sys = System::new_all();
        let smi = AllSmi::new()?;
        let mut disks = Disks::new_with_refreshed_list();
        let mut networks = Networks::new_with_refreshed_list();

        let started_at = Instant::now();
        let mut last_update = Instant::now();

        self.init_cpu(&sys);
        self.detect_cpus(&sys);
        self.detect_gpus(&smi);
        self.detect_mem(&sys);
        self.detect_disks(&disks);
        networks.refresh(true);
        self.update_networks(&networks);

        self.system = Machine {
            os: System::name(),
            version: System::os_version(),
            kernel: System::kernel_version(),
            name: System::host_name(),
            uptime: System::uptime(),
            boot: System::boot_time(),
        };

        while self.state != AppState::Quitting {
            if last_update.elapsed() >= Duration::from_secs(1) {
                let elapsed = started_at.elapsed().as_secs_f64();
                self.system.uptime = System::uptime();
                self.update_cpus(&mut sys, elapsed);
                self.update_gpus(&smi, elapsed);
                self.update_mem(&mut sys);
                self.processes(&mut sys);
                self.update_disks(&mut disks);
                networks.refresh(true);
                self.update_networks(&networks);
                last_update = Instant::now();
            }

            terminal.draw(|frame| self.render(frame))?;

            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
            {
                self.handle_key_events(key);
                if self.config_dirty {
                    save_cfg(&self.config)?;
                    self.config_dirty = false;
                }
            }
        }

        Ok(())
    }

    fn handle_key_events(&mut self, key: KeyEvent) {
        let quit_key: char = self.config.keybinds.quit;
        let sys_key: char = self.config.keybinds.system;
        let cpu_key: char = self.config.keybinds.processor;
        let gpu_key: char = self.config.keybinds.graphics;
        let disk_key: char = self.config.keybinds.disk;
        let prc_key: char = self.config.keybinds.processes;
        let mem_key: char = self.config.keybinds.memory;
        let net_key: char = self.config.keybinds.network;
        let cfg_key: char = self.config.keybinds.config;
        if self.state == AppState::Running {
            match (&self.device, key.code) {
                (DeviceSelector::Processor, KeyCode::Char(k))
                    if key.kind == event::KeyEventKind::Press && k == cpu_key =>
                {
                    self.device = DeviceSelector::None;
                }

                (DeviceSelector::Graphics, KeyCode::Char(k))
                    if key.kind == event::KeyEventKind::Press && k == gpu_key =>
                {
                    self.device = DeviceSelector::None;
                }

                (DeviceSelector::Processes, KeyCode::Char(k))
                    if key.kind == event::KeyEventKind::Press && k == prc_key =>
                {
                    self.device = DeviceSelector::None;
                }

                (DeviceSelector::Memory, KeyCode::Char(k))
                    if key.kind == event::KeyEventKind::Press && k == mem_key =>
                {
                    self.device = DeviceSelector::None;
                }

                (DeviceSelector::Disk, KeyCode::Char(k))
                    if key.kind == event::KeyEventKind::Press && k == disk_key =>
                {
                    self.device = DeviceSelector::None;
                }

                (DeviceSelector::System, KeyCode::Char(k))
                    if key.kind == event::KeyEventKind::Press && k == sys_key =>
                {
                    self.device = DeviceSelector::None;
                }

                (DeviceSelector::Network, KeyCode::Char(k))
                    if key.kind == event::KeyEventKind::Press && k == net_key =>
                {
                    self.device = DeviceSelector::None;
                }

                (DeviceSelector::Processor, KeyCode::Right)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    self.next_cpu_group();
                }

                (DeviceSelector::Processor, KeyCode::Left)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    self.previous_cpu_group();
                }

                (DeviceSelector::Graphics, KeyCode::Right)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if !self.gpus.is_empty() {
                        self.gpu_selection = (self.gpu_selection + 1) % self.gpus.len();
                    }
                }

                (DeviceSelector::Graphics, KeyCode::Left)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if !self.gpus.is_empty() {
                        if self.gpu_selection == 0 {
                            self.gpu_selection = self.gpus.len() - 1;
                        } else {
                            self.gpu_selection = (self.gpu_selection - 1) % self.gpus.len();
                        }
                    }
                }

                (DeviceSelector::Network, KeyCode::Right)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if !self.networks.is_empty() {
                        self.network_selection = (self.network_selection + 1) % self.networks.len();
                    }
                }

                (DeviceSelector::Network, KeyCode::Left)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if !self.networks.is_empty() {
                        if self.network_selection == 0 {
                            self.network_selection = self.networks.len() - 1;
                        } else {
                            self.network_selection -= 1;
                        }
                    }
                }

                (DeviceSelector::Processes, KeyCode::Up)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if !self.processes.is_empty() {
                        let last = self.processes.len() - 1;

                        let previous = match self.list_state.selected() {
                            Some(0) | None => last,
                            Some(index) => index - 1,
                        };

                        self.list_state.select(Some(previous));
                        self.mem_offset = previous + 1;
                    }
                }

                (DeviceSelector::Processes, KeyCode::Down)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if !self.processes.is_empty() {
                        let last = self.processes.len() - 1;

                        let next = match self.list_state.selected() {
                            Some(index) if index >= last => 0,
                            Some(index) => index + 1,
                            None => 0,
                        };

                        self.list_state.select(Some(next));
                        self.mem_offset = next + 1;
                    }
                }

                (DeviceSelector::Processes, KeyCode::Enter)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if let Some(index) = self.list_state.selected() {
                        self.process_selection =
                            self.processes.get(index).map(|process| process.pid);
                    }
                }

                (DeviceSelector::Processes, KeyCode::Right)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if self.process_selection.is_some() && !self.processes.is_empty() {
                        let current = self.list_state.selected().unwrap_or_default();
                        let next = current.saturating_add(1);

                        self.list_state.select(Some(next));
                        self.process_selection = Some(self.processes[next].pid);
                    }
                }

                (DeviceSelector::Processes, KeyCode::Left)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if self.process_selection.is_some() && !self.processes.is_empty() {
                        let current = self.list_state.selected().unwrap_or_default();
                        let previous = current.saturating_sub(1);

                        self.list_state.select(Some(previous));
                        self.process_selection = Some(self.processes[previous].pid);
                    }
                }

                (DeviceSelector::Processes, KeyCode::Esc)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if self.process_selection.is_some() {
                        self.process_selection = None
                    } else {
                        self.device = DeviceSelector::None
                    }
                }

                (DeviceSelector::Memory, KeyCode::Char('+'))
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if self.memory.prec_count < 4 {
                        self.memory.prec_count += 1;
                    } else {
                        self.memory.prec_count = 0;
                    }
                }

                (DeviceSelector::Memory, KeyCode::Char('-'))
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if self.memory.prec_count == 0 {
                        self.memory.prec_count = 3;
                    } else {
                        self.memory.prec_count -= 1;
                    }
                }

                (DeviceSelector::Disk, KeyCode::Right)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if !self.disks.is_empty() {
                        self.disk_selection = (self.disk_selection + 1) % self.disks.len();
                    }
                }

                (DeviceSelector::Disk, KeyCode::Left) if key.kind == event::KeyEventKind::Press => {
                    if !self.disks.is_empty() {
                        if self.disk_selection == 0 {
                            self.disk_selection = self.disks.len() - 1;
                        } else {
                            self.disk_selection = (self.disk_selection - 1) % self.disks.len();
                        }
                    }
                }

                (_, KeyCode::Char(k)) if key.kind == event::KeyEventKind::Press && k == cpu_key => {
                    self.device = DeviceSelector::Processor;
                }

                (_, KeyCode::Char(k)) if key.kind == event::KeyEventKind::Press && k == gpu_key => {
                    self.device = DeviceSelector::Graphics;
                }

                (_, KeyCode::Char(k)) if key.kind == event::KeyEventKind::Press && k == prc_key => {
                    self.device = DeviceSelector::Processes;
                    if !self.processes.is_empty() {
                        self.list_state.select(Some(0));
                    }
                }

                (_, KeyCode::Char(k)) if key.kind == event::KeyEventKind::Press && k == mem_key => {
                    self.device = DeviceSelector::Memory;
                }

                (_, KeyCode::Char(k))
                    if key.kind == event::KeyEventKind::Press && k == disk_key =>
                {
                    self.device = DeviceSelector::Disk;
                }

                (_, KeyCode::Char(k)) if key.kind == event::KeyEventKind::Press && k == sys_key => {
                    self.device = DeviceSelector::System;
                }

                (_, KeyCode::Char(k)) if key.kind == event::KeyEventKind::Press && k == net_key => {
                    self.network_selection = self
                        .network_selection
                        .min(self.networks.len().saturating_sub(1));
                    self.device = DeviceSelector::Network;
                }

                (_, KeyCode::Char(k))
                    if key.kind == event::KeyEventKind::Press && k == quit_key =>
                {
                    self.state = AppState::Quitting;
                }

                (_, KeyCode::Char(k)) if key.kind == event::KeyEventKind::Press && k == cfg_key => {
                    self.state = AppState::Config;
                }

                (_, KeyCode::Esc) if key.kind == event::KeyEventKind::Press => {
                    self.device = DeviceSelector::None;
                    self.process_selection = None;
                }

                _ => {}
            }
        } else if self.state == AppState::Config {
            match (&self.cfg_state, key.code) {
                (ConfigState::Keybinds, KeyCode::Char('k'))
                    if key.kind == event::KeyEventKind::Press =>
                {
                    self.cfg_state = ConfigState::None;
                }

                (ConfigState::Keybinds, KeyCode::Up) if key.kind == event::KeyEventKind::Press => {
                    let last = self.config.keybinds.len() - 1;

                    let previous = match self.config_key_state.selected() {
                        Some(0) | None => last,
                        Some(index) => index - 1,
                    };

                    self.config_key_state.select(Some(previous));
                }

                (ConfigState::Keybinds, KeyCode::Down)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    let last = self.config.keybinds.len() - 1;

                    let next = match self.config_key_state.selected() {
                        Some(index) if index >= last => 0,
                        Some(index) => index + 1,
                        None => 0,
                    };

                    self.config_key_state.select(Some(next));
                }

                (ConfigState::Keybinds, KeyCode::Enter)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if self.config_key_state.selected().is_some() {
                        self.keybind_error = None;
                        self.cfg_state = ConfigState::KeybindInput;
                    }
                }

                (ConfigState::KeybindInput, KeyCode::Char(input))
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if let Some(index) = self.config_key_state.selected() {
                        match self.config.keybinds.set(index, input) {
                            Ok(()) => {
                                self.config_dirty = true;
                                self.keybind_error = None;
                                self.cfg_state = ConfigState::Keybinds;
                            }
                            Err(error) => self.keybind_error = Some(error),
                        }
                    }
                }

                (ConfigState::KeybindInput, KeyCode::Esc)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    self.keybind_error = None;
                    self.cfg_state = ConfigState::Keybinds;
                }

                (ConfigState::Colors, KeyCode::Char('c'))
                    if key.kind == event::KeyEventKind::Press =>
                {
                    self.cfg_state = ConfigState::None;
                }

                (ConfigState::Colors, KeyCode::Up) if key.kind == event::KeyEventKind::Press => {
                    let last = self.config.colors.len() - 1;

                    let previous = match self.config_col_state.selected() {
                        Some(0) | None => last,
                        Some(index) => index - 1,
                    };

                    self.config_col_state.select(Some(previous));
                }

                (ConfigState::Colors, KeyCode::Down) if key.kind == event::KeyEventKind::Press => {
                    let last = self.config.colors.len() - 1;

                    let next = match self.config_col_state.selected() {
                        Some(index) if index >= last => 0,
                        Some(index) => index + 1,
                        None => 0,
                    };

                    self.config_col_state.select(Some(next));
                }

                (ConfigState::Colors, KeyCode::Enter) if key.kind == event::KeyEventKind::Press => {
                    if let Some(index) = self.config_col_state.selected() {
                        self.color_target = match index {
                            0 => Some(ColorTarget::Disk),
                            1 => Some(ColorTarget::Processes),
                            2 => Some(ColorTarget::Memory),
                            3 => Some(ColorTarget::Network),
                            _ => None,
                        };
                        self.cfg_state = ConfigState::ColorPicker;
                        self.config_edit_col_state.select(Some(0));
                    }
                }

                (ConfigState::ColorPicker, KeyCode::Up)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    let last = ConfigColor::len() - 1;

                    let previous = match self.config_edit_col_state.selected() {
                        Some(0) | None => last,
                        Some(index) => index - 1,
                    };

                    self.config_edit_col_state.select(Some(previous));
                }

                (ConfigState::ColorPicker, KeyCode::Down)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    let last = ConfigColor::len() - 1;

                    let next = match self.config_edit_col_state.selected() {
                        Some(index) if index >= last => 0,
                        Some(index) => index + 1,
                        None => 0,
                    };

                    self.config_edit_col_state.select(Some(next));
                }

                (ConfigState::ColorPicker, KeyCode::Enter)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if let Some(index) = self.config_edit_col_state.selected() {
                        let color = ConfigColor::select(index);

                        match color {
                            ConfigColor::Rgb(_, _, _) => {
                                self.color_input.clear();
                                self.color_input_error = None;
                                self.cfg_state = ConfigState::ColorInput(ColorInputKind::Rgb);
                            }
                            ConfigColor::Indexed(_) => {
                                self.color_input.clear();
                                self.color_input_error = None;
                                self.cfg_state = ConfigState::ColorInput(ColorInputKind::Indexed);
                            }
                            _ => {
                                self.apply_color(color);
                                self.finish_color_picker();
                            }
                        }
                    }
                }

                (ConfigState::ColorInput(_), KeyCode::Backspace)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    self.color_input.pop();
                    self.color_input_error = None;
                }

                (ConfigState::ColorInput(kind), KeyCode::Enter)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    match parse_color_input(*kind, &self.color_input) {
                        Ok(color) => {
                            self.apply_color(color);
                            self.finish_color_picker();
                        }
                        Err(error) => self.color_input_error = Some(error),
                    }
                }

                (ConfigState::ColorInput(_), KeyCode::Esc)
                    if key.kind == event::KeyEventKind::Press =>
                {
                    self.color_input.clear();
                    self.color_input_error = None;
                    self.cfg_state = ConfigState::ColorPicker;
                }

                (ConfigState::ColorInput(_), KeyCode::Char(input))
                    if key.kind == event::KeyEventKind::Press =>
                {
                    if input.is_ascii_digit() || input == ',' || input == ' ' {
                        self.color_input.push(input);
                        self.color_input_error = None;
                    } else {
                        self.color_input_error =
                            Some(String::from("use numbers separated by commas"));
                    }
                }

                (_, KeyCode::Esc) if key.kind == event::KeyEventKind::Press => {
                    self.state = AppState::Running;
                }

                (_, KeyCode::Char(k))
                    if key.kind == event::KeyEventKind::Press && k == quit_key =>
                {
                    self.state = AppState::Quitting;
                }

                (_, KeyCode::Char('k')) if key.kind == event::KeyEventKind::Press => {
                    self.cfg_state = ConfigState::Keybinds;
                    self.config_key_state.select(Some(0));
                }

                (_, KeyCode::Char('c')) if key.kind == event::KeyEventKind::Press => {
                    self.cfg_state = ConfigState::Colors;
                    self.config_col_state.select(Some(0));
                }

                _ => {}
            }
        }
    }

    fn apply_color(&mut self, color: ConfigColor) {
        if let Some(target) = self.color_target {
            match target {
                ColorTarget::Disk => self.config.colors.disk = color,
                ColorTarget::Processes => self.config.colors.processes = color,
                ColorTarget::Memory => self.config.colors.memory = color,
                ColorTarget::Network => self.config.colors.network = color,
            }

            self.config_dirty = true;
        }
    }

    fn finish_color_picker(&mut self) {
        self.color_target = None;
        self.color_input.clear();
        self.color_input_error = None;
        self.cfg_state = ConfigState::Colors;
        self.config_col_state.select(Some(0));
    }

    fn cpu_thread_count(&self) -> usize {
        self.cpus.len().saturating_sub(1)
    }

    fn current_cpu_group_size(&self) -> usize {
        if self.cpu_group_size == 0 {
            DEFAULT_CPU_GROUP_SIZE
        } else {
            self.cpu_group_size
        }
    }

    fn next_cpu_group(&mut self) {
        let thread_count = self.cpu_thread_count();
        if thread_count == 0 {
            self.cpu_group_start = 0;
            return;
        }

        let group_size = self.current_cpu_group_size();
        let next_group = self.cpu_group_start + group_size;
        self.cpu_group_start = if next_group >= thread_count {
            0
        } else {
            next_group
        };
    }

    fn previous_cpu_group(&mut self) {
        let thread_count = self.cpu_thread_count();
        if thread_count == 0 {
            self.cpu_group_start = 0;
            return;
        }

        let group_size = self.current_cpu_group_size();
        self.cpu_group_start = if self.cpu_group_start < group_size {
            ((thread_count - 1) / group_size) * group_size
        } else {
            self.cpu_group_start - group_size
        };
    }

    fn init_cpu(&mut self, sys: &System) {
        if let Some(cpu) = sys.cpus().first() {
            self.cpu = CpuInfo {
                brand: cpu.vendor_id().to_string(),
                model: cpu.brand().to_string(),
                core_count: System::physical_core_count().unwrap_or_default(),
                thread_count: sys.cpus().len(),
                arch: System::cpu_arch(),
            };
        }
    }

    fn detect_cpus(&mut self, sys: &System) {
        if !sys.cpus().is_empty() {
            let global_cpu = Processor {
                thread: String::from("Global"),
                usage: f64::from(sys.global_cpu_usage()),
                history: Vec::new(),
            };
            self.cpus.push(global_cpu);
        }
        for cpu in sys.cpus() {
            let device = Processor {
                thread: cpu.name().to_string(),
                usage: f64::from(cpu.cpu_usage()),
                history: Vec::new(),
            };
            self.cpus.push(device);
        }
    }

    fn update_cpus(&mut self, sys: &mut System, elapsed: f64) {
        sys.refresh_cpu_all();

        let global_usage = f64::from(sys.global_cpu_usage());

        for (processor, cpu) in self.cpus.iter_mut().skip(1).zip(sys.cpus()) {
            let usage = f64::from(cpu.cpu_usage());

            processor.usage = usage;
            processor.history.push((elapsed, usage));

            if processor.history.len() > HISTORY_SAMPLES {
                processor.history.remove(0);
            }
        }

        if let Some(global_cpu) = self.cpus.first_mut() {
            global_cpu.usage = global_usage;
            global_cpu.history.push((elapsed, global_usage));

            if global_cpu.history.len() > HISTORY_SAMPLES {
                global_cpu.history.remove(0);
            }
        }
    }

    fn detect_gpus(&mut self, smi: &AllSmi) {
        for gpu in smi.get_gpu_info() {
            let device = Gpu {
                uuid: gpu.uuid,
                name: gpu.name,
                usage: gpu.utilization,
                temp: gpu.temperature,
                history: Vec::new(),
                total_vram_bytes: gpu.total_memory,
                used_vram_bytes: gpu.used_memory,
                power: gpu.power_consumption,
            };
            self.gpus.push(device);
        }
    }

    fn update_gpus(&mut self, smi: &AllSmi, elapsed: f64) {
        let fresh_vals = smi.get_gpu_info();
        for device in &mut self.gpus {
            let Some(fresh) = fresh_vals.iter().find(|fresh| {
                if device.uuid.is_empty() {
                    fresh.name == device.name
                } else {
                    fresh.uuid == device.uuid
                }
            }) else {
                continue;
            };

            device.usage = fresh.utilization;
            device.temp = fresh.temperature;
            device.history.push((elapsed, device.usage));
            device.used_vram_bytes = fresh.used_memory;
            device.power = fresh.power_consumption;

            if device.history.len() >= HISTORY_SAMPLES {
                device.history.remove(0);
            }
        }
    }

    fn detect_mem(&mut self, sys: &System) {
        self.memory = Memory {
            capacity: sys.total_memory(),
            free: sys.free_memory(),
            used: sys.used_memory(),
            swap: Swap {
                capacity: sys.total_swap(),
                free: sys.free_swap(),
                used: sys.used_swap(),
            },
            prec_count: 2,
        };
    }

    fn update_mem(&mut self, sys: &mut System) {
        sys.refresh_memory();

        self.memory.free = sys.free_memory();
        self.memory.used = sys.used_memory();
        self.memory.swap = Swap {
            capacity: sys.total_swap(),
            free: sys.free_swap(),
            used: sys.used_swap(),
        };
    }

    fn processes(&mut self, sys: &mut System) {
        self.processes.clear();
        sys.refresh_processes(ProcessesToUpdate::All, true);

        for process in sys.processes().values() {
            let name = process.name().to_str().unwrap_or("unknown process name");
            let disk_usage = process.disk_usage();

            self.processes.push(Process {
                pid: process.pid(),
                parent_pid: process
                    .parent()
                    .map_or_else(|| "No Parent PID".to_string(), |id| id.to_string()),
                name: name.to_string(),
                memory_bytes: process.memory(),
                virtual_memory_bytes: process.virtual_memory(),
                cpu_usage: process.cpu_usage(),
                read_bytes: disk_usage.read_bytes,
                total_read_bytes: disk_usage.total_read_bytes,
                write_bytes: disk_usage.written_bytes,
                total_write_bytes: disk_usage.total_written_bytes,
                runtime: process.run_time(),
                boot: process.start_time(),
                status: process.status().to_string(),
                exe: process.exe().map_or_else(
                    || "Error: could not find executable path".to_string(),
                    |path| path.display().to_string(),
                ),
                cmd: process
                    .cmd()
                    .first()
                    .and_then(|cmd| cmd.to_str())
                    .unwrap_or("Unknown Path")
                    .to_string(),
                user: process
                    .user_id()
                    .map_or_else(|| "No ID".to_string(), |id| id.to_string()),
            });
        }

        self.processes
            .sort_by_key(|process| Reverse(process.memory_bytes));
    }

    fn detect_disks(&mut self, disks: &Disks) {
        self.disks = disks
            .iter()
            .map(|disk| Disko {
                name: disk.name().to_string_lossy().into_owned(),
                kind: format!("{:?}", disk.kind()),
                fs: disk.file_system().to_string_lossy().into_owned(),
                mnt: disk.mount_point().display().to_string(),
                usage: DiskUsage {
                    read_bytes: disk.usage().read_bytes,
                    total_read_bytes: disk.usage().total_read_bytes,
                    written_bytes: disk.usage().written_bytes,
                    total_written_bytes: disk.usage().total_written_bytes,
                },
                capacity: disk.total_space(),
                free: disk.available_space(),
            })
            .collect();
    }

    fn update_disks(&mut self, disks: &mut Disks) {
        for (pull_from, send_to) in disks.iter_mut().zip(self.disks.iter_mut()) {
            pull_from.refresh();

            send_to.usage.read_bytes = pull_from.usage().read_bytes;
            send_to.usage.total_read_bytes = pull_from.usage().total_read_bytes;
            send_to.usage.written_bytes = pull_from.usage().written_bytes;
            send_to.usage.total_written_bytes = pull_from.usage().total_written_bytes;

            send_to.free = pull_from.available_space();
        }
    }

    fn update_networks(&mut self, networks: &Networks) {
        self.networks = networks
            .iter()
            .map(|(name, network)| NetworkInterface {
                name: name.clone(),
                state: network.operational_state().to_string(),
                mac_address: network.mac_address().to_string(),
                ip_addresses: network
                    .ip_networks()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                received_bytes: network.received(),
                total_received_bytes: network.total_received(),
                transmitted_bytes: network.transmitted(),
                total_transmitted_bytes: network.total_transmitted(),
                received_packets: network.packets_received(),
                transmitted_packets: network.packets_transmitted(),
            })
            .collect();

        self.networks
            .sort_by(|left, right| left.name.cmp(&right.name));
        self.network_selection = self
            .network_selection
            .min(self.networks.len().saturating_sub(1));
    }
}

impl App {
    fn render_network_panel(&self, frame: &mut Frame, area: Rect) {
        let network_color = Color::from(self.config.colors.network);
        let network_style = match self.device {
            DeviceSelector::Network => network_color,
            _ => Color::White,
        };

        if let Some(network) = self.networks.get(self.network_selection) {
            let (received, received_unit) = format_bytes(network.received_bytes);
            let (total_received, total_received_unit) = format_bytes(network.total_received_bytes);
            let (transmitted, transmitted_unit) = format_bytes(network.transmitted_bytes);
            let (total_transmitted, total_transmitted_unit) =
                format_bytes(network.total_transmitted_bytes);

            let network_lines = vec![
                Line::from(vec![
                    Span::styled("Interface: ", Style::default().bold()),
                    Span::raw(&network.name),
                ]),
                Line::from(vec![
                    Span::styled("State: ", Style::default().bold()),
                    Span::raw(&network.state),
                ]),
                Line::from(vec![
                    Span::styled("MAC: ", Style::default().bold()),
                    Span::raw(&network.mac_address),
                ]),
                Line::from(vec![
                    Span::styled("IP: ", Style::default().bold()),
                    Span::raw(if network.ip_addresses.is_empty() {
                        "None"
                    } else {
                        &network.ip_addresses
                    }),
                ]),
                Line::from(vec![
                    Span::styled("Received: ", Style::default().bold()),
                    Span::raw(format!("{received:.2} {received_unit}/s | ")),
                    Span::raw(format!("{total_received:.2} {total_received_unit} total")),
                ]),
                Line::from(vec![
                    Span::styled("Transmitted: ", Style::default().bold()),
                    Span::raw(format!("{transmitted:.2} {transmitted_unit}/s | ")),
                    Span::raw(format!(
                        "{total_transmitted:.2} {total_transmitted_unit} total"
                    )),
                ]),
                Line::from(vec![
                    Span::styled("Packets: ", Style::default().bold()),
                    Span::raw(format!(
                        "{} received | {} transmitted",
                        network.received_packets, network.transmitted_packets
                    )),
                ]),
            ];

            let network_title =
                indexed_title("Network", self.network_selection, self.networks.len());
            let network_info = Paragraph::new(network_lines)
                .block(Block::bordered().title(network_title).style(network_style))
                .wrap(Wrap { trim: false });
            frame.render_widget(network_info, area);
        } else {
            let network_info = Paragraph::new("No network interfaces detected")
                .block(Block::bordered().title("Network").style(network_style));
            frame.render_widget(network_info, area);
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        if self.state == AppState::Config {
            let config_layout = Layout::default()
                .direction(Vertical)
                .margin(1)
                .constraints([Constraint::Length(2), Constraint::Fill(1)])
                .split(area);

            let tooltip = Paragraph::new(vec![
                Line::from(Span::styled(
                    format!("Selection: {}", self.cfg_state.name()),
                    Style::default().bold(),
                )),
                Line::from(
                    self.cfg_state
                        .keybind_description(self.config.keybinds.quit),
                ),
            ])
            .alignment(Alignment::Center);
            frame.render_widget(tooltip, config_layout[0]);

            let panels = Layout::default()
                .direction(Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(config_layout[1]);

            let keybinds_list: Vec<ListItem> = self
                .config
                .keybinds
                .iter()
                .map(|(name, key)| ListItem::new(format!("{name} => {key}")))
                .collect();

            let keybind_title = match (&self.cfg_state, &self.keybind_error) {
                (ConfigState::KeybindInput, Some(error)) => {
                    format!("Keybinds - {error}")
                }
                _ => String::from("Keybinds"),
            };

            let list = List::new(keybinds_list)
                .block(Block::bordered().title(keybind_title))
                .highlight_style(Modifier::REVERSED)
                .highlight_symbol("> ");
            frame.render_stateful_widget(list, panels[0], &mut self.config_key_state);

            match self.color_target {
                Some(device) => {
                    if let ConfigState::ColorInput(kind) = &self.cfg_state {
                        let prompt = match kind {
                            ColorInputKind::Rgb => "RGB values (example: 255,128,0)",
                            ColorInputKind::Indexed => "Palette index (0-255)",
                        };
                        let mut lines = vec![
                            Line::from(prompt),
                            Line::from(format!("> {}", self.color_input)),
                        ];
                        if let Some(error) = &self.color_input_error {
                            lines.push(Line::from(Span::styled(
                                error,
                                Style::default().fg(Color::Red),
                            )));
                        }

                        let input = Paragraph::new(lines).block(
                            Block::bordered().title(format!("Change color for: {device:?}")),
                        );
                        frame.render_widget(input, panels[1]);
                    } else {
                        let color_items: Vec<ListItem> = ConfigColor::iter()
                            .map(|color| ListItem::new(color.picker_label()))
                            .collect();

                        let color_list = List::new(color_items)
                            .block(Block::bordered().title(format!("Change color for: {device:?}")))
                            .highlight_style(Modifier::REVERSED)
                            .highlight_symbol("> ");
                        frame.render_stateful_widget(
                            color_list,
                            panels[1],
                            &mut self.config_edit_col_state,
                        );
                    }
                }
                None => {
                    let col_configs_list: Vec<ListItem> = self
                        .config
                        .colors
                        .iter()
                        .map(|(name, color)| ListItem::new(format!("{name} => {color:?}")))
                        .collect();
                    let list = List::new(col_configs_list)
                        .block(Block::bordered().title("Colors"))
                        .highlight_style(Modifier::REVERSED)
                        .highlight_symbol("> ");
                    frame.render_stateful_widget(list, panels[1], &mut self.config_col_state);
                }
            }
        } else {
            let largest = Layout::default()
                .direction(Vertical)
                .margin(1)
                .constraints(vec![
                    Constraint::Percentage(4),
                    Constraint::Percentage(49),
                    Constraint::Percentage(47),
                ])
                .split(area);

            let lines = vec![
                Line::from(vec![Span::styled(
                    format!("Selection: {}", self.device.name()),
                    Style::default().bold(),
                )]),
                Line::from(self.device.keybind_description(&self.config)),
            ];
            let top_bar = Paragraph::new(lines).alignment(Alignment::Center);

            frame.render_widget(top_bar, largest[0]);

            let large_upper = Layout::default()
                .direction(Horizontal)
                .margin(0)
                .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(largest[1]);

            let large_lower = Layout::default()
                .direction(Horizontal)
                .margin(0)
                .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(largest[2]);

            //CPU BLOCK
            let cpu_block = Layout::default()
                .direction(Horizontal)
                .margin(0)
                .constraints(vec![Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(large_upper[0]);

            //RAM BLOCK
            let ram_block = Layout::default()
                .direction(Vertical)
                .margin(0)
                .constraints(vec![Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(large_upper[1]);

            let ram_top = Layout::default()
                .direction(Horizontal)
                .margin(0)
                .constraints(vec![
                    Constraint::Max(20),
                    Constraint::Max(20),
                    Constraint::Fill(1),
                ])
                .split(ram_block[0]);

            let ram_bottom = Layout::default()
                .direction(Horizontal)
                .margin(0)
                .constraints(vec![Constraint::Percentage(25), Constraint::Percentage(75)])
                .split(ram_block[1]);

            let used_inner = Layout::default()
                .direction(Horizontal)
                .margin(0)
                .constraints(vec![
                    Constraint::Percentage(13),
                    Constraint::Percentage(80),
                    Constraint::Percentage(7),
                ])
                .split(ram_top[0]);

            let ram_right = Layout::default()
                .direction(Horizontal)
                .margin(0)
                .constraints(vec![Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(ram_top[2]);

            let ram_swap = Layout::default()
                .direction(Vertical)
                .margin(0)
                .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(ram_right[0]);

            let top_processes = Layout::default()
                .direction(Vertical)
                .margin(1)
                .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(ram_right[1]);

            let used_swap_inner = Layout::default()
                .direction(Horizontal)
                .margin(0)
                .constraints(vec![
                    Constraint::Percentage(13),
                    Constraint::Percentage(80),
                    Constraint::Percentage(7),
                ])
                .split(ram_top[1]);

            //GPU BLOCK
            let gpu_block = Layout::default()
                .direction(Horizontal)
                .margin(0)
                .constraints(vec![Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(large_lower[0]);

            //CPU SECTORS

            if let Some(global_cpu) = self.cpus.first() {
                let cpu_color = if self.cpu.brand.to_uppercase().contains("AMD") {
                    Color::Red
                } else if self.cpu.brand.to_uppercase().contains("INTEL") {
                    Color::Blue
                } else {
                    Color::White
                };

                let latest_time = global_cpu
                    .history
                    .last()
                    .map(|(time, _)| *time)
                    .unwrap_or(0.0);

                let x_end = latest_time.max(60.0);
                let x_start = x_end - 60.0;
                let x_middle = (x_start + x_end) / 2.0;

                let cpu_usage_dataset = Dataset::default()
                    .name("GLOBAL CPU")
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Style::default().fg(cpu_color))
                    .data(&global_cpu.history);

                let cpu_usage_chart = Chart::new(vec![cpu_usage_dataset])
                    .block(Block::bordered().title("CPU").style(match self.device {
                        DeviceSelector::Processor => cpu_color,
                        _ => Color::White,
                    }))
                    .x_axis(
                        Axis::default()
                            .title("Time(s)")
                            .bounds([x_start, x_end])
                            .labels([
                                format!("{x_start:.0}s"),
                                format!("{x_middle:.0}s"),
                                format!("{x_end:.0}s"),
                            ])
                            .style(Color::White),
                    )
                    .y_axis(
                        Axis::default()
                            .title("Usage (%)")
                            .bounds([0.0, 100.0])
                            .labels(["0%", "50%", "100%"])
                            .style(Color::White),
                    );
                frame.render_widget(cpu_usage_chart, cpu_block[0]);

                let system_name_check = self
                    .system
                    .os
                    .as_deref()
                    .unwrap_or("UNKNOWN OS")
                    .to_ascii_uppercase();
                let system_color = if system_name_check.contains("WINDOWS") {
                    Color::LightBlue
                } else if system_name_check.contains("LINUX") {
                    Color::Yellow
                } else if system_name_check.contains("MACOS") {
                    Color::Gray
                } else {
                    Color::LightCyan
                };

                let sys_name = &self
                    .system
                    .os
                    .as_deref()
                    .unwrap_or("Unknown OS")
                    .to_string();
                let sys_vers_raw = &self
                    .system
                    .version
                    .as_deref()
                    .unwrap_or("Unknown OS Version")
                    .to_string();
                let sys_vers = sys_vers_raw
                    .split_once("(")
                    .map(|(before, _)| before)
                    .unwrap_or(sys_vers_raw)
                    .to_string()
                    .replace(['(', ')'], "");
                let sys_kernel_raw = &self
                    .system
                    .kernel
                    .as_deref()
                    .unwrap_or("Unknown Kernel")
                    .to_string();
                let sys_kernel = sys_kernel_raw
                    .split_once("(")
                    .map(|(_, after)| after)
                    .unwrap_or(sys_kernel_raw)
                    .to_string()
                    .replace(['(', ')'], "");
                let sys_host = &self
                    .system
                    .name
                    .as_deref()
                    .unwrap_or("Unknown Name")
                    .to_string();
                let sys_uptime = time_fmt(self.system.uptime);
                let sys_boot = date_fmt(self.system.boot);
                let sys_lines = vec![
                    Line::from(vec![
                        Span::styled("OS: ", Style::default().bold()),
                        Span::raw(sys_name),
                    ]),
                    Line::from(vec![
                        Span::styled("Version: ", Style::default().bold()),
                        Span::raw(sys_vers),
                    ]),
                    Line::from(vec![
                        Span::styled("Kernel: ", Style::default().bold()),
                        Span::raw(sys_kernel),
                    ]),
                    Line::from(vec![
                        Span::styled("Host: ", Style::default().bold()),
                        Span::raw(sys_host),
                    ]),
                    Line::from(vec![
                        Span::styled("Uptime: ", Style::default().bold()),
                        Span::raw(sys_uptime),
                    ]),
                    Line::from(vec![
                        Span::styled("Booted: ", Style::default().bold()),
                        Span::raw(sys_boot),
                    ]),
                ];
                let sys_info = Paragraph::new(sys_lines).wrap(Wrap { trim: false }).block(
                    Block::bordered()
                        .title("System Info")
                        .style(match self.device {
                            DeviceSelector::System => system_color,
                            _ => Color::White,
                        }),
                );
                let cpu_lines = vec![
                    Line::from(vec![
                        Span::styled("Model: ", Style::default().bold()),
                        Span::raw(&self.cpu.model),
                    ]),
                    Line::from(vec![
                        Span::styled("Brand: ", Style::default().bold()),
                        Span::raw(&self.cpu.brand),
                    ]),
                    Line::from(vec![
                        Span::styled("Cores: ", Style::default().bold()),
                        Span::raw(self.cpu.core_count.to_string()),
                    ]),
                    Line::from(vec![
                        Span::styled("Threads: ", Style::default().bold()),
                        Span::raw(self.cpu.thread_count.to_string()),
                    ]),
                    Line::from(vec![
                        Span::styled("Arch: ", Style::default().bold()),
                        Span::raw(&self.cpu.arch),
                    ]),
                ];
                let cpu_info = Paragraph::new(cpu_lines).wrap(Wrap { trim: false }).block(
                    Block::bordered()
                        .style(match self.device {
                            DeviceSelector::Processor => cpu_color,
                            _ => Color::White,
                        })
                        .title("CPU Info"),
                );
                let info_width = cpu_block[1].width.saturating_sub(2).max(1);
                let cpu_info_height = cpu_info.line_count(info_width) as u16;
                let sys_info_height = sys_info.line_count(info_width) as u16;
                let cpu_info_vert = Layout::default()
                    .direction(Vertical)
                    .margin(0)
                    .constraints(vec![
                        Constraint::Length(cpu_info_height),
                        Constraint::Fill(1),
                        Constraint::Length(sys_info_height),
                    ])
                    .split(cpu_block[1]);

                frame.render_widget(sys_info, cpu_info_vert[2]);
                frame.render_widget(cpu_info, cpu_info_vert[0]);

                let thread_count = self.cpu_thread_count();
                let core_load_area = cpu_info_vert[1];
                let available_rows = usize::from(core_load_area.height.saturating_sub(2)).max(1);
                let group_size = available_rows.min(thread_count.max(1));
                self.cpu_group_size = group_size;

                if thread_count > 0 {
                    let last_group_start = ((thread_count - 1) / group_size) * group_size;
                    self.cpu_group_start = (self.cpu_group_start / group_size)
                        .saturating_mul(group_size)
                        .min(last_group_start);
                } else {
                    self.cpu_group_start = 0;
                }

                let group_start = self.cpu_group_start;
                let group_end = (group_start + group_size).min(thread_count);
                let group_count = thread_count.div_ceil(group_size);
                let current_group = if thread_count == 0 {
                    0
                } else {
                    group_start / group_size + 1
                };
                let visible_processors = self
                    .cpus
                    .iter()
                    .skip(group_start + 1)
                    .take(group_size)
                    .collect::<Vec<_>>();

                let core_load_title = if thread_count == 0 {
                    String::from("CORE LOAD")
                } else {
                    format!(
                        "CORE LOAD {}-{} / {} ({}/{})",
                        group_start + 1,
                        group_end,
                        thread_count,
                        current_group,
                        group_count,
                    )
                };

                let core_load_block =
                    Block::bordered()
                        .title(core_load_title)
                        .style(match self.device {
                            DeviceSelector::Processor => cpu_color,
                            _ => Color::White,
                        });
                let core_load_inner = core_load_block.inner(core_load_area);
                frame.render_widget(core_load_block, core_load_area);

                if !visible_processors.is_empty() {
                    let core_rows = Layout::default()
                        .direction(Vertical)
                        .constraints(vec![Constraint::Length(1); visible_processors.len()])
                        .split(core_load_inner);
                    let label_width = visible_processors
                        .iter()
                        .map(|processor| processor.thread.chars().count())
                        .max()
                        .unwrap_or(0)
                        .saturating_add(1);
                    let label_width = u16::try_from(label_width).unwrap_or(u16::MAX);

                    for (processor, row) in visible_processors.iter().zip(core_rows.iter()) {
                        let usage = processor.usage.clamp(0.0, 100.0);
                        let columns = Layout::default()
                            .direction(Horizontal)
                            .constraints([
                                Constraint::Length(label_width),
                                Constraint::Length(7),
                                Constraint::Fill(1),
                            ])
                            .split(*row);

                        let core_label = Paragraph::new(processor.thread.as_str())
                            .style(Style::default().fg(Color::White).bold());
                        let core_value = Paragraph::new(format!("{usage:.1}% "))
                            .alignment(Alignment::Right)
                            .style(Style::default().fg(Color::White).bold());
                        let core_gauge = Gauge::default()
                            .gauge_style(cpu_color)
                            .ratio(usage / 100.0)
                            .label("");

                        frame.render_widget(core_label, columns[0]);
                        frame.render_widget(core_value, columns[1]);
                        frame.render_widget(core_gauge, columns[2]);
                    }
                }
            } else {
                let error_msg =
                    Paragraph::new("No CPU detected").block(Block::bordered().title("CPU"));
                frame.render_widget(error_msg, large_upper[0]);
            }

            //RAM SECTORS
            let mem_color = Color::from(self.config.colors.memory);
            let mem_style = match self.device {
                DeviceSelector::Memory => mem_color,
                _ => Color::White,
            };
            //Raw Mem
            let used_mem = percentage(self.memory.used, self.memory.capacity);
            let bar_mem = Bar::default()
                .value(used_mem)
                .style(mem_style)
                .label(Line::from(format!("{used_mem}%")));

            let used_mem_chart = BarChart::vertical(vec![bar_mem])
                .bar_width(used_inner[1].width.saturating_sub(2))
                .bar_gap(0)
                .max(100);
            frame.render_widget(used_mem_chart, used_inner[1]);

            let bar_chart_outer = Block::bordered().title("RAM USAGE (%)").style(mem_style);
            frame.render_widget(bar_chart_outer, ram_top[0]);

            //Swap

            let title = "SWAP USAGE (%)";
            if self.memory.swap.used == 0 {
                let no_swap_block = Paragraph::new("Swap inactive...")
                    .block(Block::bordered().title(title).style(mem_style));
                frame.render_widget(no_swap_block, ram_top[1]);
            } else {
                let swap_used = percentage(self.memory.swap.used, self.memory.swap.capacity);
                let bar_swap = Bar::default()
                    .value(swap_used)
                    .style(mem_style)
                    .label(Line::from(format!("{swap_used}%")));

                let used_swap_chart = BarChart::vertical(vec![bar_swap])
                    .bar_width(used_inner[1].width.saturating_sub(2))
                    .bar_gap(0)
                    .max(100);
                frame.render_widget(used_swap_chart, used_swap_inner[1]);

                let bar_chart_outer = Block::bordered().title(title).style(mem_style);
                frame.render_widget(bar_chart_outer, ram_top[1]);
            }

            let (capacity, cap_unit) = format_bytes(self.memory.capacity);
            let (used, used_unit) = format_bytes(self.memory.used);
            let (free, free_unit) = format_bytes(self.memory.free);

            let ram_lines = vec![
                Line::from(vec![
                    Span::styled("Capacity: ", Style::default().bold()),
                    Span::raw(format!(
                        "{:.prec$} {}",
                        capacity,
                        cap_unit,
                        prec = self.memory.prec_count
                    )),
                ]),
                Line::from(vec![Span::styled("Live: ", Style::default().bold())]),
                Line::from(vec![
                    Span::raw(" - In use: "),
                    Span::raw(format!(
                        "{:.prec$} {}",
                        used,
                        used_unit,
                        prec = self.memory.prec_count
                    )),
                ]),
                Line::from(vec![
                    Span::raw(" - Free: "),
                    Span::raw(format!(
                        "{:.prec$} {}",
                        free,
                        free_unit,
                        prec = self.memory.prec_count
                    )),
                ]),
            ];

            let ram_info = Paragraph::new(ram_lines)
                .wrap(Wrap { trim: false })
                .block(Block::bordered().title("RAM").style(mem_style));
            frame.render_widget(ram_info, ram_swap[0]);

            let swap_used = self.memory.swap.used;
            let (capacity, cap_unit) = format_bytes(self.memory.swap.capacity);
            let (used, used_unit) = format_bytes(swap_used);
            let (free, free_unit) = format_bytes(self.memory.swap.free);

            let swap_lines = if swap_used == 0 {
                vec![
                    Line::from(vec![
                        Span::styled("Capacity: ", Style::default().bold()),
                        Span::raw(format!(
                            "{:.prec$} {}",
                            capacity,
                            cap_unit,
                            prec = self.memory.prec_count
                        )),
                    ]),
                    Line::from(vec![Span::raw("System swap currently inactive... ")]),
                ]
            } else {
                vec![
                    Line::from(vec![
                        Span::styled("Capacity: ", Style::default().bold()),
                        Span::raw(format!(
                            "{:.prec$} {}",
                            capacity,
                            cap_unit,
                            prec = self.memory.prec_count
                        )),
                    ]),
                    Line::from(vec![Span::styled("Live: ", Style::default().bold())]),
                    Line::from(vec![
                        Span::raw(" - In use: "),
                        Span::raw(format!(
                            "{:.prec$} {}",
                            used,
                            used_unit,
                            prec = self.memory.prec_count
                        )),
                    ]),
                    Line::from(vec![
                        Span::raw(" - Free: "),
                        Span::raw(format!(
                            "{:.prec$} {}",
                            free,
                            free_unit,
                            prec = self.memory.prec_count
                        )),
                    ]),
                ]
            };

            let swap_info = Paragraph::new(swap_lines)
                .wrap(Wrap { trim: false })
                .block(Block::bordered().title("SWAP").style(mem_style));
            frame.render_widget(swap_info, ram_swap[1]);

            //GPU SECTORS
            let gpu_index = self.gpu_selection.min(self.gpus.len().saturating_sub(1));
            if let Some(selected_gpu) = self.gpus.get(gpu_index) {
                let gpu_title = indexed_title("GPU", gpu_index, self.gpus.len());
                let gpu_info_title = indexed_title("GPU Info", gpu_index, self.gpus.len());
                let gpu_temp_title = indexed_title("GPU Temp (°C)", gpu_index, self.gpus.len());
                let gpu_color = if selected_gpu.name.to_uppercase().contains("NVIDIA") {
                    Color::Green
                } else if selected_gpu.name.to_uppercase().contains("AMD") {
                    Color::Red
                } else if selected_gpu.name.to_uppercase().contains("INTEL") {
                    Color::Blue
                } else {
                    Color::LightMagenta
                };

                let data = &selected_gpu.history;

                let name = format!("GPU {}", gpu_index + 1);

                let gpu_dataset_usage = Dataset::default()
                    .name(name)
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Style::default().fg(gpu_color))
                    .data(data);

                let latest_time = selected_gpu
                    .history
                    .last()
                    .map(|(time, _)| *time)
                    .unwrap_or(0.0);

                let x_end = latest_time.max(60.0);
                let x_start = x_end - 60.0;
                let x_middle = (x_start + x_end) / 2.0;

                let gpu_usage_chart = Chart::new(vec![gpu_dataset_usage])
                    .block(Block::bordered().title(gpu_title).style(match self.device {
                        DeviceSelector::Graphics => gpu_color,
                        _ => Color::White,
                    }))
                    .x_axis(
                        Axis::default()
                            .title("Time(s)")
                            .bounds([x_start, x_end])
                            .labels([
                                format!("{x_start:.0}s"),
                                format!("{x_middle:.0}s"),
                                format!("{x_end:.0}s"),
                            ])
                            .style(Color::White),
                    )
                    .y_axis(
                        Axis::default()
                            .title("Usage (%)")
                            .bounds([0.0, 100.0])
                            .labels(["0%", "50%", "100%"])
                            .style(Color::White),
                    );
                frame.render_widget(gpu_usage_chart, gpu_block[0]);

                let (used_vram, used_unit) = format_bytes(selected_gpu.used_vram_bytes);
                let (total_vram, total_unit) = format_bytes(selected_gpu.total_vram_bytes);

                let gpu_lines = vec![
                    Line::from(vec![
                        Span::styled("GPU: ", Style::default().bold()),
                        Span::raw(&selected_gpu.name),
                    ]),
                    Line::from(vec![
                        Span::styled("UUID: ", Style::default().bold()),
                        Span::raw(&selected_gpu.uuid),
                    ]),
                    Line::from(vec![
                        Span::styled("VRAM: ", Style::default().bold()),
                        Span::raw(format!("{used_vram:.2} {used_unit}")),
                        Span::raw(" / "),
                        Span::raw(format!("{total_vram:.2} {total_unit}")),
                    ]),
                    Line::from(vec![
                        Span::styled("Power Consumption: ", Style::default().bold()),
                        Span::raw(format!("{:.1}W", selected_gpu.power)),
                    ]),
                ];

                let gpu_info = Paragraph::new(gpu_lines).wrap(Wrap { trim: false }).block(
                    Block::bordered()
                        .title(gpu_info_title)
                        .style(match self.device {
                            DeviceSelector::Graphics => gpu_color,
                            _ => Color::White,
                        }),
                );
                let gpu_info_height =
                    gpu_info.line_count(gpu_block[1].width.saturating_sub(2).max(1)) as u16;
                let gpu_info_vert = Layout::default()
                    .direction(Vertical)
                    .margin(0)
                    .constraints(vec![
                        Constraint::Length(gpu_info_height),
                        Constraint::Length(5),
                        Constraint::Min(9),
                    ])
                    .split(gpu_block[1]);
                let gpu_gauge_block = Layout::default()
                    .direction(Vertical)
                    .margin(1)
                    .constraints(vec![
                        Constraint::Percentage(15),
                        Constraint::Percentage(70),
                        Constraint::Percentage(15),
                    ])
                    .split(gpu_info_vert[1]);

                frame.render_widget(gpu_info, gpu_info_vert[0]);

                let temp = selected_gpu.temp;
                let gauge_bound =
                    Block::bordered()
                        .title(gpu_temp_title)
                        .style(match self.device {
                            DeviceSelector::Graphics => gpu_color,
                            _ => Color::White,
                        });
                frame.render_widget(gauge_bound, gpu_info_vert[1]);

                let gpu_temp = Gauge::default()
                    .gauge_style(gpu_color)
                    .ratio((temp as f64) / 100.0)
                    .label(temp.to_string());
                frame.render_widget(gpu_temp, gpu_gauge_block[1]);

                self.render_network_panel(frame, gpu_info_vert[2]);
            } else {
                let error_msg =
                    Paragraph::new("No GPU detected").block(Block::bordered().title("GPU"));
                frame.render_widget(error_msg, gpu_block[0]);
                self.render_network_panel(frame, gpu_block[1]);
            }

            //PROCESSES SECTORS
            let prc_color = Color::from(self.config.colors.processes);
            match self.process_selection {
                Some(pid) => {
                    if let Some((process_index, process)) = self
                        .processes
                        .iter()
                        .enumerate()
                        .find(|(_, process)| process.pid == pid)
                    {
                        let process_title =
                            indexed_title(&process.name, process_index, self.processes.len());
                        let path = textwrap::wrap(&process.exe, 60).join("\n");

                        let command = textwrap::wrap(&process.cmd, 60).join("\n");

                        let (memory, unit_mem) = format_bytes(process.memory_bytes);
                        let (virtual_memory, unit_virt) =
                            format_bytes(process.virtual_memory_bytes);
                        let (read, unit_read) = format_bytes(process.read_bytes);
                        let (total_read, unit_total_read) = format_bytes(process.total_read_bytes);
                        let (write, unit_write) = format_bytes(process.write_bytes);
                        let (total_write, unit_total_write) =
                            format_bytes(process.total_write_bytes);

                        let rows = vec![
                            Row::new(vec![
                                Cell::from("PID:"),
                                Cell::from(process.pid.to_string()),
                            ]),
                            Row::new(vec![
                                Cell::from("Parent:"),
                                Cell::from(process.parent_pid.as_str()),
                            ]),
                            Row::new(vec![
                                Cell::from("CPU:"),
                                Cell::from(format!("{:.2} %", process.cpu_usage,)),
                            ]),
                            Row::new(vec![
                                Cell::from("Memory:"),
                                Cell::from(format!("{:.2} {}", memory, unit_mem)),
                            ]),
                            Row::new(vec![
                                Cell::from("Virtual:"),
                                Cell::from(format!("{:.2} {}", virtual_memory, unit_virt)),
                            ]),
                            Row::new(vec![
                                Cell::from("Reading:"),
                                Cell::from(format!("{:.2} {}/s", read, unit_read)),
                            ]),
                            Row::new(vec![
                                Cell::from("Total Read:"),
                                Cell::from(format!("{:.2} {}", total_read, unit_total_read)),
                            ]),
                            Row::new(vec![
                                Cell::from("Writing:"),
                                Cell::from(format!("{:.2} {}/s", write, unit_write)),
                            ]),
                            Row::new(vec![
                                Cell::from("Total Write:"),
                                Cell::from(format!("{:.2} {}", total_write, unit_total_write)),
                            ]),
                            Row::new(vec![
                                Cell::from("Status:"),
                                Cell::from(process.status.as_str()),
                            ]),
                            Row::new(vec![
                                Cell::from("Runtime:"),
                                Cell::from(time_fmt(process.runtime)),
                            ]),
                            Row::new(vec![
                                Cell::from("Started:"),
                                Cell::from(date_fmt(process.boot)),
                            ]),
                            Row::new(vec![Cell::from("Path:"), Cell::from(path)]).height(2),
                            Row::new(vec![Cell::from("Command:"), Cell::from(command)]).height(2),
                            Row::new(vec![
                                Cell::from("UserID:"),
                                Cell::from(process.user.as_str()),
                            ]),
                        ];

                        let widths = [Constraint::Length(12), Constraint::Min(10)];

                        let table = Table::new(rows, widths)
                            .style(Style::default().fg(Color::White))
                            .block(
                                Block::bordered()
                                    .style(Style::default().fg(prc_color))
                                    .title(process_title),
                            );

                        frame.render_widget(table, large_lower[1]);
                    }
                }
                None => {
                    let width = large_lower[1].width as usize;
                    let processes: Vec<ListItem> = self
                        .processes
                        .iter()
                        .map(|process| {
                            let (memory, unit) = format_bytes(process.memory_bytes);
                            let usage = format!("{memory:.2} {unit}");
                            let name = process.name.as_str();

                            let spaces = if self.mem_offset == 0 {
                                width.saturating_sub(name.len() + usage.len() + 3)
                            } else {
                                width.saturating_sub(name.len() + usage.len() + 5)
                            };

                            let line = Line::from(vec![
                                Span::raw(name),
                                Span::raw(" ".repeat(spaces)),
                                Span::raw(usage),
                            ]);

                            ListItem::new(line)
                        })
                        .collect();

                    let list = if processes.is_empty() {
                        vec![ListItem::new(Span::raw(
                            "Attempting to retrieve processes...".to_string(),
                        ))]
                    } else {
                        processes
                    };
                    let list = List::new(list)
                        .block(
                            Block::bordered()
                                .title("Processes")
                                .style(match self.device {
                                    DeviceSelector::Processes => prc_color,
                                    _ => Color::White,
                                }),
                        )
                        .style(Color::White)
                        .highlight_style(Modifier::REVERSED)
                        .highlight_symbol("> ");
                    frame.render_stateful_widget(list, large_lower[1], &mut self.list_state);
                }
            }

            let outer_block = Block::bordered().style(match self.device {
                DeviceSelector::Processes => prc_color,
                _ => Color::White,
            });
            frame.render_widget(outer_block, ram_right[1]);
            let mut top_cpu: Vec<&Process> = self.processes.iter().collect();
            top_cpu.sort_by(|a, b| b.cpu_usage.total_cmp(&a.cpu_usage));
            let mut lines = vec![Line::from(vec![Span::styled(
                "Top CPU: ",
                Style::default().bold(),
            )])];
            for (rank, process) in top_cpu.iter().take(5).enumerate() {
                let pos = rank + 1;
                let line = Line::from(vec![
                    Span::styled(format!("{} - ", pos), Style::default().bold()),
                    Span::raw(format!("{} : {:.1}%", process.name, process.cpu_usage)),
                ]);
                lines.push(line);
            }
            let top_cpu_block = Paragraph::new(lines);
            frame.render_widget(top_cpu_block, top_processes[0]);

            let mut top_io: Vec<&Process> = self.processes.iter().collect();
            top_io.sort_by(|a, b| {
                let t_a = a.read_bytes.saturating_add(a.write_bytes);
                let t_b = b.read_bytes.saturating_add(b.write_bytes);

                t_b.cmp(&t_a)
            });

            let mut lines = vec![Line::from(vec![Span::styled(
                "Top I/O: ",
                Style::default().bold(),
            )])];
            for (rank, process) in top_io.iter().take(5).enumerate() {
                let pos = rank + 1;
                let io_bytes = process.read_bytes.saturating_add(process.write_bytes);
                let (io_rate, unit) = format_bytes(io_bytes);
                let line = Line::from(vec![
                    Span::styled(format!("{} - ", pos), Style::default().bold()),
                    Span::raw(format!("{} : {:.2} {}/s", process.name, io_rate, unit)),
                ]);
                lines.push(line);
            }
            let top_io_block = Paragraph::new(lines);
            frame.render_widget(top_io_block, top_processes[1]);

            //LOGO BLOCK
            let logo_block = Block::bordered();
            frame.render_widget(logo_block, ram_bottom[0]);
            //

            //DISK SECTION
            let disk_color = Color::from(self.config.colors.disk);
            let selected_disk = &self.disks[self.disk_selection];
            let disk_title = indexed_title("Disk Info", self.disk_selection, self.disks.len());

            let (read, r_unit) = format_bytes(selected_disk.usage.read_bytes);
            let (t_read, tr_unit) = format_bytes(selected_disk.usage.total_read_bytes);
            let (write, w_unit) = format_bytes(selected_disk.usage.written_bytes);
            let (t_write, tw_unit) = format_bytes(selected_disk.usage.total_written_bytes);

            let (cap, c_unit) = format_bytes(selected_disk.capacity);
            let (free, f_unit) = format_bytes(selected_disk.free);

            let disk_lines = vec![
                Line::from(vec![
                    Span::styled("Name: ", Style::default().bold()),
                    Span::raw(&selected_disk.name),
                ]),
                Line::from(vec![
                    Span::styled("Type: ", Style::default().bold()),
                    Span::raw(&selected_disk.kind),
                ]),
                Line::from(vec![
                    Span::styled("File System: ", Style::default().bold()),
                    Span::raw(&selected_disk.fs),
                ]),
                Line::from(vec![
                    Span::styled("Mount Point: ", Style::default().bold()),
                    Span::raw(&selected_disk.mnt),
                ]),
                Line::from(vec![
                    Span::styled("Capacity: ", Style::default().bold()),
                    Span::raw(format!("{:.2} {}", free, f_unit)),
                    Span::raw(" / "),
                    Span::raw(format!("{:.2} {}", cap, c_unit)),
                ]),
                Line::from(vec![
                    Span::styled("Read: ", Style::default().bold()),
                    Span::raw(format!("{:.2} {}/s", read, r_unit)),
                    Span::raw(" (current) | "),
                    Span::raw(format!("{:.2} {}", t_read, tr_unit)),
                    Span::raw(" (total)"),
                ]),
                Line::from(vec![
                    Span::styled("Write: ", Style::default().bold()),
                    Span::raw(format!("{:.2} {}/s", write, w_unit)),
                    Span::raw(" (current) | "),
                    Span::raw(format!("{:.2} {}", t_write, tw_unit)),
                    Span::raw(" (total)"),
                ]),
            ];

            let disk_block = Paragraph::new(disk_lines)
                .block(
                    Block::bordered()
                        .title(disk_title)
                        .style(match self.device {
                            DeviceSelector::Disk => disk_color,
                            _ => Color::White,
                        }),
                )
                .wrap(Wrap { trim: false });

            frame.render_widget(disk_block, ram_bottom[1]);
        }
    }
}

#[cfg(test)]
mod app_tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use ratatui::{Terminal, backend::TestBackend};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn indexed_titles_show_the_current_and_total_items() {
        assert_eq!(indexed_title("GPU", 0, 3), "GPU (1/3)");
        assert_eq!(indexed_title("Disk Info", 2, 3), "Disk Info (3/3)");
        assert_eq!(indexed_title("Network", 0, 0), "Network");
    }

    #[test]
    fn config_tooltip_tracks_the_current_editing_state() {
        let mut app = App {
            state: AppState::Config,
            ..App::default()
        };
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| app.render(frame)).unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .concat();
        assert!(rendered.contains("Selection: Config"));
        assert!(rendered.contains("Keybinds (k) | Colors (c)"));

        app.cfg_state = ConfigState::KeybindInput;
        terminal.draw(|frame| app.render(frame)).unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .concat();
        assert!(rendered.contains("Selection: Edit Keybind"));
        assert!(rendered.contains("Press a character | Cancel (Esc)"));
    }

    #[test]
    fn color_picker_collects_rgb_and_indexed_values() {
        let rgb_index = ConfigColor::iter()
            .position(|color| matches!(color, ConfigColor::Rgb(_, _, _)))
            .unwrap();
        let indexed_index = ConfigColor::iter()
            .position(|color| matches!(color, ConfigColor::Indexed(_)))
            .unwrap();
        let mut app = App {
            state: AppState::Config,
            cfg_state: ConfigState::ColorPicker,
            color_target: Some(ColorTarget::Disk),
            ..App::default()
        };

        app.config_edit_col_state.select(Some(rgb_index));
        app.handle_key_events(key(KeyCode::Enter));
        assert_eq!(app.cfg_state, ConfigState::ColorInput(ColorInputKind::Rgb));
        for input in "12,34,56".chars() {
            app.handle_key_events(key(KeyCode::Char(input)));
        }
        app.handle_key_events(key(KeyCode::Enter));
        assert!(matches!(
            app.config.colors.disk,
            ConfigColor::Rgb(12, 34, 56)
        ));

        app.cfg_state = ConfigState::ColorPicker;
        app.color_target = Some(ColorTarget::Network);
        app.config_edit_col_state.select(Some(indexed_index));
        app.handle_key_events(key(KeyCode::Enter));
        for input in "201".chars() {
            app.handle_key_events(key(KeyCode::Char(input)));
        }
        app.handle_key_events(key(KeyCode::Enter));
        assert!(matches!(
            app.config.colors.network,
            ConfigColor::Indexed(201)
        ));
        assert!(app.config_dirty);
    }

    #[test]
    fn custom_color_input_validates_shape_and_range() {
        assert_eq!(
            parse_color_input(ColorInputKind::Rgb, "1,2").unwrap_err(),
            "enter exactly three values: R,G,B"
        );
        assert_eq!(
            parse_color_input(ColorInputKind::Rgb, "256,2,3").unwrap_err(),
            "values must be whole numbers from 0 to 255"
        );
        assert_eq!(
            parse_color_input(ColorInputKind::Indexed, "1,2").unwrap_err(),
            "enter one palette index"
        );
    }

    #[test]
    fn network_key_selects_the_panel_and_arrows_cycle_interfaces() {
        let mut app = App {
            networks: vec![
                NetworkInterface {
                    name: "Ethernet".to_string(),
                    ..NetworkInterface::default()
                },
                NetworkInterface {
                    name: "Wi-Fi".to_string(),
                    ..NetworkInterface::default()
                },
            ],
            ..App::default()
        };

        app.handle_key_events(key(KeyCode::Char(app.config.keybinds.network)));
        assert_eq!(app.device, DeviceSelector::Network);

        app.handle_key_events(key(KeyCode::Right));
        assert_eq!(app.network_selection, 1);

        app.handle_key_events(key(KeyCode::Left));
        assert_eq!(app.network_selection, 0);
    }

    #[test]
    fn cpu_arrows_page_by_available_height_and_wrap() {
        let mut app = App {
            device: DeviceSelector::Processor,
            disks: vec![Disko::default()],
            ..App::default()
        };
        app.cpus.push(Processor {
            thread: "Global".to_string(),
            ..Processor::default()
        });
        app.cpus.extend((0..20).map(|index| Processor {
            thread: format!("CPU {index}"),
            usage: 7.0,
            ..Processor::default()
        }));

        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        assert!(app.cpu_group_size > DEFAULT_CPU_GROUP_SIZE);
        assert!(app.cpu_group_size < app.cpu_thread_count());

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .concat();
        assert!(rendered.contains("7.0%"));
        assert!(rendered.chars().any(|symbol| "▏▎▍▌▋▊▉█".contains(symbol)));

        let group_size = app.cpu_group_size;
        let last_group = ((app.cpu_thread_count() - 1) / group_size) * group_size;

        app.handle_key_events(key(KeyCode::Right));
        assert_eq!(app.cpu_group_start, group_size);

        app.handle_key_events(key(KeyCode::Left));
        assert_eq!(app.cpu_group_start, 0);

        app.handle_key_events(key(KeyCode::Left));
        assert_eq!(app.cpu_group_start, last_group);

        app.handle_key_events(key(KeyCode::Right));
        assert_eq!(app.cpu_group_start, 0);
    }
}
