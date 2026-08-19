#![allow(unused)]

use std::{
    time::{
        Duration,
        Instant,
    },
    thread
};

use sysinfo::{
    Components, Disks, Networks, System,
};

use all_smi::{AllSmi, Result as SmiResult};

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use ratatui::{DefaultTerminal};
use ratatui::buffer::Buffer;
use ratatui::layout::Direction::{Horizontal, Vertical};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::palette::tailwind;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Padding, Paragraph, Widget, Axis, Chart, GraphType, Dataset};
use ratatui::symbols::Marker;

#[derive(Debug, Default)]
struct App {
    state : AppState,
    device : DeviceSelector,
    system : Machine,
    cpus : Vec<Cpu>,
    gpus : Vec<Gpu>,
    cpu_selection : usize,
    gpu_selection : usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum AppState {
    #[default]
    Running,
    Started,
    Quitting,
}

#[derive(Debug, Default)]
enum DeviceSelector {
    #[default]
    None,
    Processor,
    Graphics,
    Disk,
}

#[derive(Debug, Default)]
struct Machine {
    name : Option<String>,
    uptime : u64,
}

#[derive(Debug, Default)]
struct Cpu {
    thread : String,
    brand : String,
    usage : f64,
    frequency : f64,
    core_count : usize,
    history : Vec<(f64, f64)>,
}

#[derive(Debug, Default)]
struct Gpu {
    name : String,
    usage : f64,
    temp : u32,
    history : Vec<(f64, f64)>,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    ratatui::run(|terminal| App::default().run(terminal))
}

impl App {
    fn run(&mut self, terminal : &mut DefaultTerminal) -> Result<()> {
        let mut sys = System::new_all();
        let smi = AllSmi::new()?;

        let started_at = Instant::now();
        let mut last_update = Instant::now();

        self.update_sys(&sys);
        self.detect_cpu(&sys);
        self.detect_gpus(&smi);

        while self.state != AppState::Quitting {

            if last_update.elapsed() >= Duration::from_secs(1) {
                let elapsed = started_at.elapsed().as_secs_f64();
                self.update_sys(&sys);
                self.update_cpu(&mut sys, elapsed);
                self.update_gpus(&smi, elapsed);
                last_update = Instant::now();
            }
            
            terminal.draw(|frame| frame.render_widget(&*self, frame.area()))?;

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

            (_, KeyCode::Up) if key.kind == event::KeyEventKind::Press => {
                self.device = DeviceSelector::Processor;
            }

            (_, KeyCode::Down) if key.kind == event::KeyEventKind::Press => {
                self.device = DeviceSelector::Graphics;
            }

            (_, KeyCode::Char('q')) => {
                self.state = AppState::Quitting;
            }

            (_, KeyCode::Esc) if key.kind == event::KeyEventKind::Press => {
                self.device = DeviceSelector::None;
            }

            _ => {}
        }
    }

    fn update_sys(&mut self, sys : &System) {
        let system = Machine {
            name : System::name(),
            uptime : System::uptime(),
        };
        self.system = system;
    }

    fn time_fmt(&self, seconds : u64) -> String {
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

    fn detect_cpu(&mut self, sys : &System) {
        for cpu in sys.cpus() {
            let device = Cpu {
                thread : cpu.name().to_string(),
                brand : cpu.vendor_id().to_string(),
                usage : cpu.cpu_usage() as f64,
                frequency : cpu.frequency() as f64,
                core_count : match System::physical_core_count() {Some(count) => count, None => 0},
                history : Vec::new(),
            };
            self.cpus.push(device);
        }
    }

    fn update_cpu(&mut self, sys : &mut System, elapsed : f64) {
        sys.refresh_cpu_usage();

        for cpu in self.cpus.iter_mut() {

            let usage = sys.global_cpu_usage() as f64;
            cpu.usage = usage;
            
            cpu.history.push((elapsed, usage));
    
            const MAX_SAMPLES : usize = 60;
    
            if cpu.history.len() > MAX_SAMPLES {
                cpu.history.remove(0);
            }
        }
    }

    fn detect_gpus(&mut self, smi : &AllSmi) {
        for gpu in smi.get_gpu_info() {
            let device = Gpu {
                name : gpu.name,
                usage : gpu.utilization,
                temp : gpu.temperature,
                history : Vec::new(),
            };
            self.gpus.push(device);
        }
    }

    fn update_gpus(&mut self,smi : &AllSmi, elapsed : f64) {
        const MAX_SAMPLES :usize = 60;
        let fresh_vals = smi.get_gpu_info();
        for (device, fresh) in self.gpus.iter_mut().zip(fresh_vals.iter()) {

            device.usage = fresh.utilization;
            device.temp = fresh.temperature;
            device.history.push((elapsed, device.usage));

            if device.history.len() >= MAX_SAMPLES {
                device.history.remove(0);
            }
        }
    }


}

impl Widget for &App {
    fn render(self, area : Rect, buf : &mut Buffer) {
        use Constraint::{Length, Min, Ratio};

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

        let cpu_info_vert = Layout::default()
            .direction(Vertical)
            .margin(0)
            .constraints(vec![
                Constraint::Percentage(30),
                Constraint::Percentage(40),
                Constraint::Percentage(30),
            ])
            .split(cpu_block[1]);

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

        let gpu_info_vert = Layout::default()
            .direction(Vertical)
            .margin(0)
            .constraints(vec![
                Constraint::Percentage(30),
                Constraint::Percentage(40),
                Constraint::Percentage(30),
            ])
            .split(gpu_block[1]);

        let gpu_gauge_block = Layout::default()
            .direction(Vertical)
            .margin(1)
            .constraints(vec![
                Constraint::Percentage(30),
                Constraint::Percentage(40),
                Constraint::Percentage(30),
            ])
            .split(gpu_info_vert[0]);

        //DISK BLOCK
        // let disk_block = Layout::default()
        //     .direction(direction)


        //CPU SECTORS

        let cpu_index = self.cpu_selection.min(self.cpus.len().saturating_sub(1));
        if let Some(selected_cpu) = self.cpus.get(cpu_index) {
            let cpu_color = if selected_cpu.brand.to_uppercase().contains("AMD") {
                Color::Red
            } else if selected_cpu.brand.to_uppercase().contains("INTEL") {
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
    
            let cpu_usage_dataset = Dataset::default()
                    .name(selected_cpu.thread.as_str())
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
            cpu_usage_chart.render(cpu_block[0], buf);
            
            let sys_name = &self.system.name.as_deref().unwrap_or("Unknown OS").to_string();
            let sys_uptime = self.time_fmt(self.system.uptime);
            let text = format!("OS: {}\nUptime: {}", sys_name, sys_uptime);
            let sys_info = Paragraph::new(text)
                .block(
                    Block::bordered()
                    .title("System Info")
                    .style(match self.device {
                        DeviceSelector::Processor => {cpu_color},
                        _ => {Color::White}
                    })
                );
            sys_info.render(cpu_info_vert[0], buf);
    
            let cpu_temp = Block::bordered();
            cpu_temp.render(cpu_info_vert[1], buf);
    
            let cpu_pid = Block::bordered();
            cpu_pid.render(cpu_info_vert[2], buf);
        } else {
            let error_msg = Paragraph::new("No CPU detected")
                .block(Block::bordered().title("CPU"));
            error_msg.render(large_upper[0], buf);
        }


        //RAM SECTORS
        let ram_usage = Block::bordered();
        ram_usage.render(ram_block[0], buf);

        let ram_info = Block::bordered();
        ram_info.render(ram_block[1], buf);


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
    
            let display_name = selected_gpu.name
                .replace("NVIDIA GeForce ", "")
                .replace("Laptop GPU", "")
                .replace("AMD Radeon ", "")
                .replace("Graphics", "");
    
            let gpu_dataset_usage = Dataset::default()
                .name(display_name)
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
            gpu_usage_chart.render(gpu_block[0], buf);
    
            let temp = selected_gpu.temp;
            let gauge_bound = Block::bordered()
                .title("GPU Temp(C)")
                .style(match self.device {
                    DeviceSelector::Graphics => {gpu_color}
                    _ => {Color::White}
                });
                gauge_bound.render(gpu_info_vert[0], buf);
    
            let gpu_temp = Gauge::default()
                .gauge_style(gpu_color)
                .ratio((temp as f64) / 100.0)
                .label(temp.to_string());
            gpu_temp.render(gpu_gauge_block[1], buf);
    
            let gpu_usage = Block::bordered();
            gpu_usage.render(gpu_info_vert[1], buf);
    
            let gpu_name = Block::bordered();
            gpu_name.render(gpu_info_vert[2], buf);
        } else {
            let error_msg = Paragraph::new("No GPU detected")
                .block(Block::bordered().title("GPU"));
            error_msg.render(large_lower[0], buf);
        }


        //DISK SECTORS
        let block6 = Block::bordered();
        block6.render(large_lower[1], buf);
    }
}
