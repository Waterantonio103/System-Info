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
    cpu : Cpu,
    gpus : Vec<Gpu>,
    selection : usize,
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
    Processor,
    Graphics,
    Disk,
}

#[derive(Debug, Default)]
struct Cpu {
    brand : String,
    usage : f64,
    temp : f64,
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

        self.detect_gpus(smi);

        while self.state != AppState::Quitting {

            if last_update.elapsed() >= Duration::from_secs(1) {
                let elapsed = started_at.elapsed().as_secs_f64();
                self.update_cpu(&mut sys, elapsed);
                self.update_gpus(elapsed);
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
        match key.code {
            KeyCode::Up => {
                self.device = DeviceSelector::Processor;
            },
            KeyCode::Down => {
                self.device = DeviceSelector::Graphics;
            }
            KeyCode::Right => {
                self.device = DeviceSelector::Disk;
            }
            KeyCode::Char('q') => {
                self.state = AppState::Quitting;
            },
            _ => {}
        }
        match self.device {
            DeviceSelector::Processor if key.kind == event::KeyEventKind::Press => 
                match key.code {
                    
                    KeyCode::Char('e') => {
                        // if !self.gpus.is_empty() {
                        //     self.selection = (self.selection + 1) % self.gpus.len();
                        // }
                    }
                    _ => {}
                }
            ,
            DeviceSelector::Graphics if key.kind == event::KeyEventKind::Press => 
                match key.code {
                    KeyCode::Char('e') => {
                        if !self.gpus.is_empty() {
                            self.selection = (self.selection + 1) % self.gpus.len();
                        }
                    }
                    _ => {}
                }
            ,
            DeviceSelector::Disk if key.kind == event::KeyEventKind::Press => 
                match key.code {
                    
                    _ => {}
                }
            _ => {}
        }
    }

    fn update_cpu(&mut self, mut sys : &mut System, elapsed : f64) {
        sys.refresh_cpu_usage();

        let usage = sys.global_cpu_usage() as f64;
        self.cpu.usage = usage;
        
        self.cpu.history.push((elapsed, usage));

        const MAX_SAMPLES : usize = 60;

        if self.cpu.history.len() > MAX_SAMPLES {
            self.cpu.history.remove(0);
        }
    }

    fn detect_gpus(&mut self, smi : AllSmi) {
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

    fn update_gpus(&mut self, elapsed : f64) {
        const MAX_SAMPLES :usize = 60;
        for device in self.gpus.iter_mut() {

            let usage = device.usage;
            device.history.push((elapsed, usage));

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

        //DISK BLOCK
        // let disk_block = Layout::default()
        //     .direction(direction)


        //CPU SECTORS

        let latest_time = self 
                .cpu
                .history
                .last()
                .map(|(time, _)|*time)
                .unwrap_or(0.0);

        let x_end = latest_time.max(60.0);
        let x_start = x_end - 60.0;
        let x_middle = (x_start + x_end) / 2.0;

        let cpu_usage_dataset = Dataset::default()
                .name("CPU USAGE")
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Magenta))
                .data(&self.cpu.history);

        let cpu_usage_chart = Chart::new(vec![cpu_usage_dataset])
            .block(Block::bordered().title("CPU"))
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
        
        let cpu_name = Block::bordered();
        cpu_name.render(cpu_info_vert[0], buf);

        let cpu_temp = Block::bordered();
        cpu_temp.render(cpu_info_vert[1], buf);

        let cpu_pid = Block::bordered();
        cpu_pid.render(cpu_info_vert[2], buf);

        let mut text = String::new();
        for gpu in self.gpus.iter() {
            let name = &gpu.name;
            text += name;
        }
        //RAM SECTORS
        let ram_usage = Paragraph::new(text);
        ram_usage.render(ram_block[0], buf);

        let ram_info = Block::bordered();
        ram_info.render(ram_block[1], buf);



        //GPU SECTORS
        let selected_gpu = &self.gpus[self.selection];
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
            .style(Style::default().fg(Color::Red))
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
                    DeviceSelector::Graphics => {Color::Red},
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

        let gpu_temp = Block::bordered();
        gpu_temp.render(gpu_info_vert[0], buf);

        let gpu_usage = Block::bordered();
        gpu_usage.render(gpu_info_vert[1], buf);

        let gpu_name = Block::bordered();
        gpu_name.render(gpu_info_vert[2], buf);


        //DISK SECTORS
        let block6 = Block::bordered();
        block6.render(large_lower[1], buf);
    }
}
