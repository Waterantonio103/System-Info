use sysinfo::{DiskUsage, Pid};

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
    pub fn name(&self) -> String {
        match self {
            DeviceSelector::None => String::from("No device selected"),
            DeviceSelector::System => String::from("System"),
            DeviceSelector::Processor => String::from("Processors"),
            DeviceSelector::Graphics => String::from("Graphics Cards"),
            DeviceSelector::Disk => String::from("Disks"),
            DeviceSelector::Processes => String::from("Processes"),
            DeviceSelector::Memory => String::from("Memory"),
            DeviceSelector::Network => String::from("Networks"),
        }
    }
    pub fn keybind_description(&self) -> String {
        match self {
            DeviceSelector::None => String::from("Quit (q) | System (s) | CPU (c) | GPU (g) | Disk (d) | Processes (p) | Memory (m) | Network (n)"),
            DeviceSelector::System => String::from("Quit (q) | Back (Esc / s)"),
            DeviceSelector::Processor => String::from("Quit (q) | Back (Esc / c) | Next CPU (Right)"),
            DeviceSelector::Graphics => String::from("Quit (q) | Back (Esc / g) | Next GPU (Right)"),
            DeviceSelector::Disk => String::from("Quit (q) | Back (Esc / d)"),
            DeviceSelector::Processes => String::from("Quit (q) | Back (Esc / p) | Navigate (Up/Down) | Inspect (Enter) | Previous/Next (Left/Right)"),
            DeviceSelector::Memory => String::from("Quit (q) | Back (Esc / m) | Precision (+/-)"),
            DeviceSelector::Network => String::from("Quit (q) | Back (Esc / n)"),
        }
    }
}

#[derive(Debug, Default)]
pub struct Machine {
    pub os : Option<String>,
    pub version : Option<String>,
    pub kernel : Option<String>,
    pub name : Option<String>,
    pub uptime : u64,
    pub boot : u64,
}

#[derive(Debug, Default)]
pub struct CpuInfo {
    pub brand : String,
    pub model : String,
    pub core_count : usize,
    pub thread_count : usize,
    pub arch : String,
}

#[derive(Debug, Default)]
pub struct Processor {
    pub thread : String,
    pub usage : f64,
    pub frequency : f64,
    pub history : Vec<(f64, f64)>,
    pub freq_hist : Vec<(f64, f64)>,
}

#[derive(Debug, Default)]
pub struct Gpu {
    pub uuid : String,
    pub name : String,
    pub usage : f64,
    pub history : Vec<(f64, f64)>,
    pub temp : u32,
    pub total_vram_bytes : u64,
    pub used_vram_bytes : u64,
    pub power : f64,
}

#[derive(Debug, Default)]
pub struct Memory {
    pub capacity : f64,
    pub free : f64,
    pub used : f64,
    pub swap : Swap,
    pub prec_count : usize,
}

#[derive(Debug, Default)]
pub struct Swap {
    pub capacity : f64,
    pub free : f64,
    pub used : f64,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Process {
    pub pid : Pid,
    pub parent_pid : String,
    pub name : String,
    pub memory_bytes : u64,
    pub virtual_memory_bytes : u64,
    pub cpu_usage : f32,
    pub read_bytes : u64,
    pub total_read_bytes : u64,
    pub write_bytes : u64,
    pub total_write_bytes : u64,
    pub runtime : u64,
    pub boot : u64,
    pub status : String,
    pub exe : String,
    pub cmd : String,
    pub user : String,
}

#[derive(Debug, Default)]
pub struct Disko {
    pub name : String,
    pub kind : String,
    pub fs : String,
    pub mnt : String,
    pub usage : DiskUsage,
    pub capacity : u64,
    pub free : u64,
}
