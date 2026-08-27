#![allow(unused)]

use std::{
    fmt::format, ops::Deref, thread, time::{Duration, Instant},
};

use all_smi::{AllSmi, Result as SmiResult};
use chrono::{DateTime, Local};
use color_eyre::{Result, owo_colors::style};
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use ratatui::{
    DefaultTerminal, Frame, layout::{
        Alignment, Constraint, Direction::{Horizontal, Vertical}, Layout, Rect,
    }, style::{Color, Modifier, Style, Styled, Stylize, palette::tailwind}, symbols::Marker, text::{Line, Span}, widgets::{
        Axis, Block, Borders, Chart, Dataset, Gauge, GraphType, List, ListItem, ListState, Padding, Paragraph, Table, Row, Cell, Wrap,
    },
};
use sysinfo::{Components, Cpu, Disks, Networks, Pid, ProcessesToUpdate, System, Uid};

#[derive(Debug, Default)]
struct App {
    state : AppState,
    device : DeviceSelector,
    system : Machine,
    cpu : CpuInfo,
    cpus : Vec<Processor>,
    gpus : Vec<Gpu>,
    cpu_selection : usize,
    gpu_selection : usize,
    list_state : ListState,
    memory : Memory,
    processes : Vec<Process>,
    mem_offset : usize,
    process_selection : Option<Pid>,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum AppState {
    #[default]
    Running,
    Started,
    Quitting,
}

#[derive(Debug, Default, PartialEq)]
enum DeviceSelector {
    #[default]
    None,
    Processor,
    Graphics,
    Disk,
    Processes,
}

#[derive(Debug, Default)]
struct Machine {
    os : Option<String>,
    version : Option<String>,
    kernel : Option<String>,
    name : Option<String>,
    uptime : u64,
    boot : u64,
}

#[derive(Debug, Default)]
struct CpuInfo {
    brand : String,
    model : String,
    core_count : usize,
    thread_count : usize,
    arch : String,
}

#[derive(Debug, Default)]
struct Processor {
    thread : String,
    usage : f64,
    frequency : f64,
    history : Vec<(f64, f64)>,
    freq_hist : Vec<(f64, f64)>,
}

#[derive(Debug, Default)]
struct Gpu {
    uuid : String,
    name : String,
    usage : f64,
    history : Vec<(f64, f64)>,
    temp : u32,
    total_vram : f64,
    total_unit : String,
    used_vram : f64,
    used_unit : String,
    power : f64,
}

#[derive(Debug, Default)]
struct Memory {
    capacity : f64,
    free : f64,
    used : f64,
    swap : Swap,
}

#[derive(Debug, Default)]
struct Swap {
    capacity : f64,
    free : f64,
    used : f64,
}

#[derive(Debug, PartialEq)]
struct Process {
    pid : Pid,
    parent_pid : String,
    name : String,
    memory_bytes : f64,
    memory : f64,
    unit_mem : String,
    virtual_memory : f64,
    unit_virt : String,
    read : f64,
    unit_read : String,
    total_read : f64,
    unit_ttread : String,
    write : f64,
    unit_write : String,
    total_write : f64,
    unit_ttwrite : String,
    runtime : u64,
    boot : u64,
    status : String,
    exe : String,
    cmd : String,
    user : String,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    ratatui::run(|terminal| App::default().run(terminal))
}

fn format_bytes(bytes : f64) -> (f64, &'static str) {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * KB;
    const GB: f64 = 1024.0 * MB;
    const TB: f64 = 1024.0 * GB;

    if bytes >= TB {
            (bytes / TB , "TB")
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

fn time_fmt(seconds : u64) -> String {
    const DAY_IN_SECS : u64 = 86400;
    const HOUR_IN_SECS : u64 = 3600;
    const MIN_IN_SECS : u64 = 60;
    
    let days = seconds / DAY_IN_SECS;
    let hours = seconds / HOUR_IN_SECS;
    let minutes = seconds / MIN_IN_SECS;

    let clock = if days > 0 {
        let remaining_hrs = seconds % DAY_IN_SECS;
        let hours = remaining_hrs / HOUR_IN_SECS;
        let remaining_mins = remaining_hrs % HOUR_IN_SECS;
        let mins = remaining_mins / MIN_IN_SECS;
        let secs = remaining_mins % MIN_IN_SECS;
        format!("{:02}d{:02}h{:02}m{:02}s",days,hours,mins,secs)
    } else if hours > 0 {
        let remaining_mins = seconds % HOUR_IN_SECS;
        let mins = remaining_mins / MIN_IN_SECS;
        let secs = remaining_mins % MIN_IN_SECS;
        format!("{:02}h{:02}m{:02}s",hours,mins,secs)
    } else {
        let secs = seconds % MIN_IN_SECS;
        format!("{:02}m{:02}s",minutes,secs)
    };
    
    clock
}

fn date_fmt(boot : u64) -> String {
    let boot_date = DateTime::from_timestamp(boot as i64, 0)
        .unwrap()
        .with_timezone(&Local);

    boot_date.format("%m/%d/%Y at %I:%M:%S %p").to_string()
}

impl App {
    fn run(&mut self, terminal : &mut DefaultTerminal) -> Result<()> {
        let mut sys = System::new_all();
        let smi = AllSmi::new()?;

        let started_at = Instant::now();
        let mut last_update = Instant::now();

        self.update_sys(&sys);
        self.init_cpu(&sys);
        self.detect_cpus(&sys);
        self.detect_gpus(&smi);
        self.detect_mem(&sys);

        self.system = Machine {
            os : System::name(),
            version : System::os_version(),
            kernel : System::kernel_version(),
            name : System::host_name(),
            uptime : System::uptime(),
            boot : System::boot_time(),
        };

        while self.state != AppState::Quitting {

            if last_update.elapsed() >= Duration::from_secs(1) {
                let elapsed = started_at.elapsed().as_secs_f64();
                self.update_sys(&sys);
                self.update_cpus(&mut sys,elapsed);
                self.update_gpus(&smi, elapsed);
                self.update_mem(&mut sys);
                self.processes(&mut sys);
                last_update = Instant::now();
            }
            
            terminal.draw(|frame| self.render(frame))?;

            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key_events(key);
                }
            }
        } 

        Ok(())
    }

    fn handle_key_events(&mut self, key : KeyEvent) {
        match (&self.device, key.code) {
            (DeviceSelector::Processor, KeyCode::Right) if key.kind == event::KeyEventKind::Press => {
                if !self.cpus.is_empty() {
                    self.cpu_selection = (self.cpu_selection + 1) % self.cpus.len();
                }
            }

            (DeviceSelector::Graphics, KeyCode::Right) if key.kind == event::KeyEventKind::Press => {
                if !self.gpus.is_empty() {
                    self.gpu_selection = (self.gpu_selection + 1) % self.gpus.len();
                }
            }

            (DeviceSelector::Processes, KeyCode::Up) if key.kind == event::KeyEventKind::Press => {
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

            (DeviceSelector::Processes, KeyCode::Down) if key.kind == event::KeyEventKind::Press => {
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

            (DeviceSelector::Processes, KeyCode::Enter) if key.kind == event::KeyEventKind::Press => {
                if let Some(index) = self.list_state.selected() {
                    self.process_selection = self.processes
                        .get(index)
                        .map(|process| process.pid);
                }
            }

            (DeviceSelector::Processes, KeyCode::Right) if key.kind == event::KeyEventKind::Press => {
                if self.process_selection.is_some() && !self.processes.is_empty() {
                    let current = self.list_state.selected().unwrap_or_default();
                    let next = current.saturating_add(1);
                    
                    self.list_state.select(Some(next));
                    self.process_selection = Some(self.processes[next].pid);
                }
            }

            (DeviceSelector::Processes, KeyCode::Left) if key.kind == event::KeyEventKind::Press => {
                if self.process_selection.is_some() && !self.processes.is_empty() {
                    let current = self.list_state.selected().unwrap_or_default();
                    let previous = current.saturating_sub(1);
                    
                    self.list_state.select(Some(previous));
                    self.process_selection = Some(self.processes[previous].pid);
                }
            }

            (DeviceSelector::Processes, KeyCode::Esc) if key.kind == event::KeyEventKind::Press => {
                if self.process_selection.is_some() {
                    self.process_selection = None
                } else {
                    self.device = DeviceSelector::None
                }
            }

            (_, KeyCode::Up) if key.kind == event::KeyEventKind::Press => {
                self.device = DeviceSelector::Processor;
            }

            (_, KeyCode::Down) if key.kind == event::KeyEventKind::Press => {
                self.device = DeviceSelector::Graphics;
            }

            (_, KeyCode::Right) if key.kind == event::KeyEventKind::Press => {
                self.device = DeviceSelector::Processes;
            }

            (_, KeyCode::Char('q')) => {
                self.state = AppState::Quitting;
            }

            (_, KeyCode::Esc) if key.kind == event::KeyEventKind::Press => {
                self.device = DeviceSelector::None;
                self.process_selection = None;
            }

            _ => {}
        }
    }

    fn update_sys(&mut self, sys : &System) {
        self.system.uptime = System::uptime();
    }

    fn init_cpu(&mut self, sys : &System) {
        if let Some(cpu) = sys.cpus().first() {
            self.cpu = CpuInfo {
                brand : cpu.vendor_id().to_string(),
                model : cpu.brand().to_string(),
                core_count : System::physical_core_count().unwrap_or_default(),
                thread_count : sys.cpus().len(),
                arch : System::cpu_arch(),
            };
        }
    }

    fn detect_cpus(&mut self, sys : &System) {
        if let Some(cpu) = sys.cpus().first() {
            let global_cpu = Processor {
                thread : String::from("Global"),
                usage : sys.global_cpu_usage() as f64,
                frequency : cpu.frequency() as f64,
                history : Vec::new(),
                freq_hist : Vec::new(),
            };
            self.cpus.push(global_cpu);
        }
        for cpu in sys.cpus() {
            let device = Processor {
                thread : cpu.name().to_string(),
                usage : cpu.cpu_usage() as f64,
                frequency : cpu.frequency() as f64,
                history : Vec::new(),
                freq_hist : Vec::new(),
            };
        ;
            self.cpus.push(device);
        }
    }

    fn update_cpus(&mut self, sys : &mut System, elapsed : f64) {
        const MAX_SAMPLES: usize = 60;
        sys.refresh_cpu_all();

        let global_usage = sys.global_cpu_usage() as f64;
        let mut frequency_sum = 0.0;
        let mut frequency_count = 0;

        for (processor, cpu) in self.cpus.iter_mut().skip(1).zip(sys.cpus()) {
            let usage = cpu.cpu_usage() as f64;
            let frequency = cpu.frequency() as f64 / 1000.0; // MHz -> GHz

            processor.usage = usage;
            processor.frequency = frequency;
            processor.history.push((elapsed, usage));
            processor.freq_hist.push((elapsed, frequency));

            frequency_sum += frequency;
            frequency_count += 1;

            if processor.history.len() > MAX_SAMPLES {
                processor.history.remove(0);
            }

            if processor.freq_hist.len() > MAX_SAMPLES {
                processor.freq_hist.remove(0);
            }
        }

        let global_frequency = if frequency_count > 0 {
            frequency_sum / frequency_count as f64
        } else {
            0.0
        };

        if let Some(global_cpu) = self.cpus.first_mut() {
            global_cpu.usage = global_usage;
            global_cpu.frequency = global_frequency;
            global_cpu.history.push((elapsed, global_usage));
            global_cpu.freq_hist.push((elapsed, global_frequency));

            if global_cpu.history.len() > MAX_SAMPLES {
                global_cpu.history.remove(0);
            }

            if global_cpu.freq_hist.len() > MAX_SAMPLES {
                global_cpu.freq_hist.remove(0);
            }
        }
    }
    
    fn detect_gpus(&mut self, smi : &AllSmi) {
        for gpu in smi.get_gpu_info() {
            let (total_vram, total_unit) = format_bytes(gpu.total_memory as f64);
            let (used_vram, used_unit) = format_bytes(gpu.used_memory as f64);
            let device = Gpu {
                uuid : gpu.uuid,
                name : gpu.name,
                usage : gpu.utilization,
                temp : gpu.temperature,
                history : Vec::new(),
                total_vram : total_vram,
                total_unit : total_unit.to_string(),
                used_vram : used_vram,
                used_unit : used_unit.to_string(),
                power : gpu.power_consumption,
            };
            self.gpus.push(device);
        }
    }

    fn update_gpus(&mut self,smi : &AllSmi, elapsed : f64) {
        const MAX_SAMPLES :usize = 60;
        let fresh_vals = smi.get_gpu_info();
        for device in &mut self.gpus {
            let Some(fresh) = fresh_vals
                .iter()
                .find(|fresh| {
                    if !device.uuid.is_empty() {
                        fresh.uuid == device.uuid
                    } else {
                        fresh.name == device.name
                    }
                })
            else {
                continue;
            };

            let (fresh_used, fresh_unit) = format_bytes(fresh.used_memory as f64);
            device.usage = fresh.utilization;
            device.temp = fresh.temperature;
            device.history.push((elapsed, device.usage));
            device.used_vram = fresh_used;
            device.used_unit = fresh_unit.to_string();
            device.power = fresh.power_consumption;
            
            if device.history.len() >= MAX_SAMPLES {
                device.history.remove(0);
            }
        }
            
    }

    fn detect_mem(&mut self, sys : &System) {
        self.memory = Memory { 
            capacity: sys.total_memory() as f64, 
            free: sys.free_memory() as f64, 
            used: sys.used_memory() as f64, 
            swap: Swap { 
                capacity: sys.total_swap() as f64, 
                free: sys.free_swap() as f64, 
                used: sys.used_swap() as f64, 
            }, 
        };
    }

    fn update_mem(&mut self, sys : &mut System) {
        sys.refresh_memory();

        self.memory = Memory { 
            capacity: sys.total_memory() as f64, 
            free: sys.free_memory() as f64, 
            used: sys.used_memory() as f64, 
            swap: Swap { 
                capacity: sys.total_swap() as f64, 
                free: sys.free_swap() as f64, 
                used: sys.used_swap() as f64, 
            }, 
        };
    }

    fn processes(&mut self, sys : &mut System) {
        self.processes.clear();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        const KB: f64 = 1024.0;
        const MB: f64 = 1024.0 * KB;
        const GB: f64 = 1024.0 * MB;

        for (pid, process) in sys.processes() {
            let name = process.name().to_str().unwrap_or("unknown process name");

            let mut mem = process.memory() as f64;
            let mem_bytes = mem;

            let mut virt_mem = process.virtual_memory() as f64;

            let mut rd = process.disk_usage().read_bytes as f64;
            let mut trd = process.disk_usage().total_read_bytes as f64;
            let mut wte = process.disk_usage().written_bytes as f64;
            let mut twte = process.disk_usage().total_written_bytes as f64;

            let (memory, unit) = format_bytes(mem);
            let (virt_memory, virt_unit) = format_bytes(virt_mem);
            let (read, read_unit) = format_bytes(rd);
            let (total_read, ttread_unit) = format_bytes(trd);
            let (write, write_unit) = format_bytes(wte);
            let (total_write, ttwrite_unit) = format_bytes(twte);

            self.processes.push(Process {
                pid : process.pid(), 
                parent_pid : process.parent().map(|id| id.to_string()).unwrap_or("No Parent PID".to_string()),
                name: name.to_string(), 
                memory_bytes : mem_bytes,
                memory: memory, 
                unit_mem : unit.to_string(), 
                virtual_memory : virt_memory,
                unit_virt : virt_unit.to_string(),
                read : read,
                unit_read : read_unit.to_string(), 
                total_read : total_read,
                unit_ttread : ttread_unit.to_string(),
                write : write,
                unit_write : write_unit.to_string(),
                total_write : total_write,
                unit_ttwrite : ttwrite_unit.to_string(),
                runtime : process.run_time(),
                boot : process.start_time(),
                status : process.status().to_string(),
                exe : process.exe().map(|path| path.display().to_string()).unwrap_or("Error: could not find executable path".to_string()),
                cmd : process.cmd().first().and_then(|cmd| cmd.to_str()).unwrap_or("Unknown Path").to_string(),
                user : process.user_id().map(|id| id.to_string()).unwrap_or("No ID".to_string()),
            });
        }

        self.processes.sort_by(|a, b| {
            b.memory_bytes.partial_cmp(&a.memory_bytes).unwrap()
        });
    }

}

impl App {
    fn render(&mut self, frame : &mut Frame) {
        let area = frame.area();

        let largest = Layout::default()
            .direction(Vertical)
            .margin(1)
            .constraints(vec![
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(area);

        let large_upper = Layout::default()
            .direction(Horizontal)
            .margin(0)
            .constraints(vec![
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(largest[0]);

        let large_lower = Layout::default()
            .direction(Horizontal)
            .margin(0)
            .constraints(vec![
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(largest[1]);

        //CPU BLOCK
        let cpu_block = Layout::default()
            .direction(Horizontal)
            .margin(0)
            .constraints(vec![
                Constraint::Percentage(60),
                Constraint::Percentage(40),
            ])
            .split(large_upper[0]);

        //RAM BLOCK
        let ram_block = Layout::default()
            .direction(Vertical)
            .margin(0)
            .constraints(vec![
                Constraint::Percentage(60),
                Constraint::Percentage(40),
            ])
            .split(large_upper[1]);

        //GPU BLOCK
        let gpu_block = Layout::default()
            .direction(Horizontal)
            .margin(0)
            .constraints(vec![
                Constraint::Percentage(60),
                Constraint::Percentage(40),
            ])
            .split(large_lower[0]);

        //CPU SECTORS

        let cpu_index = self.cpu_selection.min(self.cpus.len().saturating_sub(1));
        if let Some(selected_cpu) = self.cpus.get(cpu_index) {
            let cpu_color = if self.cpu.brand.to_uppercase().contains("AMD") {
                Color::Red
            } else if self.cpu.brand.to_uppercase().contains("INTEL") {
                Color::Blue
            } else {
                Color::White
            };
    
            let latest_time = selected_cpu
                    .history
                    .last()
                    .map(|(time, _)|*time)
                    .unwrap_or(0.0);
    
            let x_end = latest_time.max(60.0);
            let x_start = x_end - 60.0;
            let x_middle = (x_start + x_end) / 2.0;
    
            let cpu_name = if self.cpu_selection == 0 {
                "GLOBAL CPU"
            } else {
                selected_cpu.thread.as_str()
            };
            let cpu_usage_dataset = Dataset::default()
                    .name(cpu_name)
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Style::default().fg(cpu_color))
                    .data(&selected_cpu.history);
    
            let cpu_usage_chart = Chart::new(vec![cpu_usage_dataset])
                .block(Block::bordered()
                    .title("CPU")
                    .style(match self.device {
                        DeviceSelector::Processor => {cpu_color}
                        _ => {Color::White}
                    })
                )
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
            
            let sys_name = &self.system.os.as_deref().unwrap_or("Unknown OS").to_string();
            let sys_vers_raw = &self.system.version.as_deref().unwrap_or("Unknown OS Version").to_string();
            let sys_vers = sys_vers_raw
                .split_once("(")
                .map(|(before, _)|before)
                .unwrap_or(sys_vers_raw)
                .to_string()
                .replace(['(', ')'], "");
            let sys_kernel_raw = &self.system.version.as_deref().unwrap_or("Unknown Kernel").to_string();
            let sys_kernel = sys_kernel_raw
                .split_once("(")
                .map(|(_, after)|after)
                .unwrap_or(sys_kernel_raw)
                .to_string()
                .replace(['(', ')'], "");
            let sys_host = &self.system.name.as_deref().unwrap_or("Unknown Name").to_string();
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
            let sys_info = Paragraph::new(sys_lines)
                .wrap(Wrap { trim: false })
                .block(
                    Block::bordered()
                    .title("System Info")
                    .style(match self.device {
                        DeviceSelector::Processor => {cpu_color},
                        _ => {Color::White}
                    })
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
            let cpu_info = Paragraph::new(cpu_lines)
                .wrap(Wrap { trim: false })
                .block(Block::bordered()
                    .style(match self.device {
                        DeviceSelector::Processor => {cpu_color},
                        _ => {Color::White}
                    })
                    .title("CPU Info")
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

            let latest_time = selected_cpu
                    .freq_hist
                    .last()
                    .map(|(time, _)|*time)
                    .unwrap_or(0.0);

            let x_end = latest_time.max(60.0);
            let x_start = x_end - 60.0;
            let x_middle = (x_start + x_end) / 2.0;

            let max_bound = selected_cpu.freq_hist.iter().map(|(_, freq)| *freq).reduce(f64::max).unwrap_or(0.0) + 0.5;
            let mid_bound = max_bound / 2.0;
            let max_label = format!("{max_bound:.1}GHz");
            let mid_label = format!("{mid_bound:.1}GHz");

            let cpu_freq_dataset = Dataset::default()
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(Style::default().fg(cpu_color))
                    .data(&selected_cpu.freq_hist);

            let cpu_freq = Chart::new(vec![cpu_freq_dataset])
                .block(Block::bordered()
                    .title("FREQUENCY")
                    .style(match self.device {
                        DeviceSelector::Processor => {cpu_color}
                        _ => {Color::White}
                    })
                )
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
                        .bounds([0.0, max_bound])
                        .labels(["0GHz", mid_label.as_str(), max_label.as_str()])
                        .style(Color::White),
                );
            frame.render_widget(cpu_freq, cpu_info_vert[1]);
    
        } else {
            let error_msg = Paragraph::new("No CPU detected")
                .block(Block::bordered().title("CPU"));
            frame.render_widget(error_msg, large_upper[0]);
        }


        //RAM SECTORS
        let ram_usage = Block::bordered();
        frame.render_widget(ram_usage, ram_block[0]);

        let ram_info = Block::bordered();
        frame.render_widget(ram_info, ram_block[1]);


        //GPU SECTORS
        let gpu_index = self.gpu_selection.min(self.gpus.len().saturating_sub(1));
        if let Some(selected_gpu) = self.gpus.get(gpu_index) {
            let gpu_color= if selected_gpu.name.to_uppercase().contains("NVIDIA") {
                Color::Green
            } else if selected_gpu.name.to_uppercase().contains("AMD") {
                Color::Red
            } else if selected_gpu.name.to_uppercase().contains("INTEL") {
                Color::Blue
            } else {
                Color::LightMagenta
            };
    
            let data = &selected_gpu.history;
    
            // let display_name = selected_gpu.name
            //     .replace("NVIDIA GeForce ", "")
            //     .replace("Laptop GPU", "")
            //     .replace("AMD Radeon ", "")
            //     .replace("Graphics", "");

            let name = format!("GPU {}", gpu_index);

            let gpu_dataset_usage = Dataset::default()
                .name(name)
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(gpu_color))
                .data(data);
    
            let latest_time = selected_gpu
                    .history
                    .last()
                    .map(|(time, _)|*time)
                    .unwrap_or(0.0);
    
            let x_end = latest_time.max(60.0);
            let x_start = x_end - 60.0;
            let x_middle = (x_start + x_end) / 2.0;
    
            let gpu_usage_chart = Chart::new(vec![gpu_dataset_usage])
                .block(Block::bordered()
                    .title("GPU")
                    .style(match self.device {
                        DeviceSelector::Graphics => {gpu_color},
                        _ => {Color::White}
                    })    
                )
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
                    Span::raw(format!("{:.1}{}",selected_gpu.used_vram, selected_gpu.used_unit)),
                    Span::raw(" / "),
                    Span::raw(format!("{:.1}{}",selected_gpu.total_vram, selected_gpu.total_unit)),
                ]),
                Line::from(vec![
                    Span::styled("Power Consumption: ", Style::default().bold()),
                    Span::raw(format!("{:.1}W", selected_gpu.power)),
                ]),
            ];

            let gpu_info = Paragraph::new(gpu_lines)
            .wrap(Wrap { trim: false })
            .block(Block::bordered()
                .title("GPU Info")
                .style(match self.device {
                    DeviceSelector::Graphics => {gpu_color}
                    _ => {Color::White}
                })
            );
            let gpu_info_height = gpu_info
                .line_count(gpu_block[1].width.saturating_sub(2).max(1)) as u16;
            let gpu_info_vert = Layout::default()
                .direction(Vertical)
                .margin(0)
                .constraints(vec![
                    Constraint::Length(gpu_info_height),
                    Constraint::Length(5),
                    Constraint::Fill(1),
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
            let gauge_bound = Block::bordered()
                .title("GPU Temp (°C)")
                .style(match self.device {
                    DeviceSelector::Graphics => {gpu_color}
                    _ => {Color::White}
                });
            frame.render_widget(gauge_bound, gpu_info_vert[1]);

            let gpu_temp = Gauge::default()
                .gauge_style(gpu_color)
                .ratio((temp as f64) / 100.0)
                .label(temp.to_string());
            frame.render_widget(gpu_temp, gpu_gauge_block[1]);
    
    
            let gpu_name = Block::bordered();
            frame.render_widget(gpu_name, gpu_info_vert[2]);
        } else {
            let error_msg = Paragraph::new("No GPU detected")
                .block(Block::bordered().title("GPU"));
            frame.render_widget(error_msg, large_lower[0]);
        }


        //PROCESSES SECTORS
        const PROCESS_COLOR : Color = Color::LightBlue;
        match self.process_selection {
            Some(pid) => {
                if let Some(process) = self.processes.iter().find(|process| process.pid == pid) {
                    let path = textwrap::wrap(&process.exe, 60)
                        .join("\n");

                    let command = textwrap::wrap(&process.cmd, 60)
                        .join("\n");

                    let rows = vec![
                        Row::new(vec![
                            Cell::from("PID:"),
                            Cell::from(process.pid.to_string()),
                        ]),

                        Row::new(vec![
                            Cell::from("Parent:"),
                            Cell::from(process.parent_pid.to_string()),
                        ]),

                        Row::new(vec![
                            Cell::from("Memory:"),
                            Cell::from(format!(
                                "{:.2} {}",
                                process.memory,
                                process.unit_mem
                            )),
                        ]),

                        Row::new(vec![
                            Cell::from("Virtual:"),
                            Cell::from(format!(
                                "{:.2} {}",
                                process.virtual_memory,
                                process.unit_virt
                            )),
                        ]),

                        Row::new(vec![
                            Cell::from("Reading:"),
                            Cell::from(format!(
                                "{:.2} {}",
                                process.read,
                                process.unit_read
                            )),
                        ]),

                        Row::new(vec![
                            Cell::from("Total Read:"),
                            Cell::from(format!(
                                "{:.2} {}",
                                process.total_read,
                                process.unit_ttread
                            )),
                        ]),

                        Row::new(vec![
                            Cell::from("Writing:"),
                            Cell::from(format!(
                                "{:.2} {}",
                                process.write,
                                process.unit_write
                            )),
                        ]),

                        Row::new(vec![
                            Cell::from("Total Write:"),
                            Cell::from(format!(
                                "{:.2} {}",
                                process.total_write,
                                process.unit_ttwrite
                            )),
                        ]),

                        Row::new(vec![
                            Cell::from("Status:"),
                            Cell::from(process.status.clone()),
                        ]),

                        Row::new(vec![
                            Cell::from("Runtime:"),
                            Cell::from(time_fmt(process.runtime)),
                        ]),

                        Row::new(vec![
                            Cell::from("Started:"),
                            Cell::from(date_fmt(process.boot)),
                        ]),

                        Row::new(vec![
                            Cell::from("Path:"),
                            Cell::from(path),
                        ])
                        .height(2),

                        Row::new(vec![
                            Cell::from("Command:"),
                            Cell::from(command),
                        ])
                        .height(2),

                        Row::new(vec![
                            Cell::from("UserID:"),
                            Cell::from(process.user.to_string()),
                        ]),
                    ];

                    let widths = [
                        Constraint::Length(12),
                        Constraint::Min(10),
                    ];

                    let table = Table::new(rows, widths)
                        .style(Style::default().fg(Color::White))
                        .block(Block::bordered().style(Style::default().fg(PROCESS_COLOR)).title(process.name.clone()));

                    frame.render_widget(table, large_lower[1]);
                }
                }
            None => {
                let width = large_lower[1].width as usize;
                let processes: Vec<ListItem> = self.processes
                    .iter()
                    .map(|process| {
                        let usage = format!("{:.1}{}", process.memory, process.unit_mem);
                        let name = process.name.clone();
        
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
                    vec![ListItem::new(Span::raw("Attempting to retrieve processes...".to_string()))]
                } else {
                    processes
                };
                let list = List::new(list)
                    .block(Block::bordered()
                        .title("Processes")
                        .style(match self.device {
                            DeviceSelector::Processes => {PROCESS_COLOR},
                            _ => {Color::White}
                        })
                    )
                    .style(Color::White)
                    .highlight_style(Modifier::REVERSED)
                    .highlight_symbol("> ");
                frame.render_stateful_widget(list, large_lower[1], &mut self.list_state);
            }
        }
    }
}
