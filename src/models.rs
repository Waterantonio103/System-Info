use sysinfo::{DiskUsage, Pid};

use crate::config::Config;

#[derive(Debug, Default, PartialEq)]
pub enum DeviceSelector {
    #[default]
    None,
    System,
    Processor,
    Graphics,
    Disk,
    Processes,
    Memory,
    Network,
}

impl DeviceSelector {
    pub fn name(&self) -> &'static str {
        match self {
            DeviceSelector::None => "No device selected",
            DeviceSelector::System => "System",
            DeviceSelector::Processor => "Processors",
            DeviceSelector::Graphics => "Graphics Cards",
            DeviceSelector::Disk => "Disks",
            DeviceSelector::Processes => "Processes",
            DeviceSelector::Memory => "Memory",
            DeviceSelector::Network => "Networks",
        }
    }
    pub fn keybind_description(&self, config: &Config) -> String {
        match self {
            DeviceSelector::None => format!(
                "Quit ({}) | System ({}) | CPU ({}) | GPU ({}) | Disk ({}) | Processes ({}) | Memory ({}) | Network ({}) | Config ({})",
                config.keybinds.quit,
                config.keybinds.system,
                config.keybinds.processor,
                config.keybinds.graphics,
                config.keybinds.disk,
                config.keybinds.processes,
                config.keybinds.memory,
                config.keybinds.network,
                config.keybinds.config,
            ),
            DeviceSelector::System => format!(
                "Quit ({}) | Back (Esc / {})",
                config.keybinds.quit, config.keybinds.system
            ),
            DeviceSelector::Processor => format!(
                "Quit ({}) | Back (Esc / {}) | Previous/Next Core Group (Left/Right)",
                config.keybinds.quit, config.keybinds.processor
            ),
            DeviceSelector::Graphics => format!(
                "Quit ({}) | Back (Esc / {}) | Previous/Next GPU (Left/Right)",
                config.keybinds.quit, config.keybinds.graphics
            ),
            DeviceSelector::Disk => format!(
                "Quit ({}) | Back (Esc / {}) | Previous/Next Disk (Left/Right)",
                config.keybinds.quit, config.keybinds.disk
            ),
            DeviceSelector::Processes => format!(
                "Quit ({}) | Back (Esc / {}) | Navigate (Up/Down) | Inspect (Enter) | Previous/Next (Left/Right)",
                config.keybinds.quit, config.keybinds.processes
            ),
            DeviceSelector::Memory => format!(
                "Quit ({}) | Back (Esc / {}) | Precision (+/-)",
                config.keybinds.quit, config.keybinds.memory
            ),
            DeviceSelector::Network => format!(
                "Quit ({}) | Back (Esc / {}) | Previous/Next Network (Left/Right)",
                config.keybinds.quit, config.keybinds.network
            ),
        }
    }
}

#[derive(Debug, Default)]
pub struct Machine {
    pub os: Option<String>,
    pub version: Option<String>,
    pub kernel: Option<String>,
    pub name: Option<String>,
    pub uptime: u64,
    pub boot: u64,
}

#[derive(Debug, Default)]
pub struct CpuInfo {
    pub brand: String,
    pub model: String,
    pub core_count: usize,
    pub thread_count: usize,
    pub arch: String,
}

#[derive(Debug, Default)]
pub struct Processor {
    pub thread: String,
    pub usage: f64,
    pub history: Vec<(f64, f64)>,
}

#[derive(Debug, Default)]
pub struct Gpu {
    pub uuid: String,
    pub name: String,
    pub usage: f64,
    pub history: Vec<(f64, f64)>,
    pub temp: u32,
    pub total_vram_bytes: u64,
    pub used_vram_bytes: u64,
    pub power: f64,
}

#[derive(Debug, Default)]
pub struct Memory {
    pub capacity: u64,
    pub free: u64,
    pub used: u64,
    pub swap: Swap,
    pub prec_count: usize,
}

#[derive(Debug, Default)]
pub struct Swap {
    pub capacity: u64,
    pub free: u64,
    pub used: u64,
}

#[derive(Debug)]
pub struct Process {
    pub pid: Pid,
    pub parent_pid: String,
    pub name: String,
    pub memory_bytes: u64,
    pub virtual_memory_bytes: u64,
    pub cpu_usage: f32,
    pub read_bytes: u64,
    pub total_read_bytes: u64,
    pub write_bytes: u64,
    pub total_write_bytes: u64,
    pub runtime: u64,
    pub boot: u64,
    pub status: String,
    pub exe: String,
    pub cmd: String,
    pub user: String,
}

#[derive(Debug, Default)]
pub struct Disko {
    pub name: String,
    pub kind: String,
    pub fs: String,
    pub mnt: String,
    pub usage: DiskUsage,
    pub capacity: u64,
    pub free: u64,
}

#[derive(Debug, Default)]
pub struct NetworkInterface {
    pub name: String,
    pub state: String,
    pub mac_address: String,
    pub ip_addresses: String,
    pub received_bytes: u64,
    pub total_received_bytes: u64,
    pub transmitted_bytes: u64,
    pub total_transmitted_bytes: u64,
    pub received_packets: u64,
    pub transmitted_packets: u64,
}
