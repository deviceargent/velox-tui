use std::time::Instant;
use ratatui::widgets::TableState;
use crate::models::{ScanResult, ProcessData, SysTickData};

// --- Navigation ---

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum ActivePane {
    Disk(usize),
    Ram,
    Power,
    Export,
    Processes,
}

// --- Lifecycle ---

#[derive(PartialEq)]
pub enum Screen {
    Splash(Instant),
    Dashboard,
}

// --- App ---

pub struct App {
    // Scan data (written by channel drains)
    pub hardware_data: Vec<ScanResult>,
    pub disk_data: Vec<ScanResult>,       // pre-filtered subset of hardware_data
    pub usage_data: Vec<ProcessData>,
    pub filtered_usage: Vec<ProcessData>, // search-filtered, rebuilt on change

    // Chart data (pre-computed, rebuilt only when history changes — not per frame)
    pub ram_history: Vec<f64>,
    pub cpu_history: Vec<f64>,
    pub ram_chart_data: Vec<(f64, f64)>,
    pub cpu_chart_data: Vec<(f64, f64)>,
    pub current_ram_pct: f64,
    pub current_cpu_pct: f64,

    // Process table
    pub table_state: TableState,
    pub selected_pid: Option<u32>, // tracks by PID so sort/refresh doesn't lose selection
    pub sort_by_cpu: bool,

    // Search
    pub search_query: String,

    // UI
    pub active_pane: ActivePane,
    pub zoomed: bool,
    pub input_mode: bool,
    pub paused: bool,

    // Kill confirmation (arms on first x, executes on second x for same PID)
    pub kill_confirm_pid: Option<u32>,

    // Export / recording
    pub export_path: String,
    pub recording: bool,
    pub last_export_error: Option<String>,

    // Lifecycle
    pub screen: Screen,
    pub host_name: String,
    pub should_quit: bool,
}

impl App {
    pub fn new(host_name: String) -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0));
        Self {
            hardware_data: Vec::new(),
            disk_data: Vec::new(),
            usage_data: Vec::new(),
            filtered_usage: Vec::new(),
            ram_history: vec![0.0],
            cpu_history: vec![0.0],
            ram_chart_data: vec![(99.0, 0.0)],
            cpu_chart_data: vec![(99.0, 0.0)],
            current_ram_pct: 0.0,
            current_cpu_pct: 0.0,
            table_state,
            selected_pid: None,
            sort_by_cpu: true,
            search_query: String::new(),
            active_pane: ActivePane::Processes,
            zoomed: false,
            input_mode: false,
            paused: false,
            kill_confirm_pid: None,
            export_path: String::from("./report.txt"),
            recording: false,
            last_export_error: None,
            screen: Screen::Splash(Instant::now()),
            host_name,
            should_quit: false,
        }
    }

    // --- Channel update handlers ---

    pub fn apply_hw_update(&mut self, hw: Vec<ScanResult>) {
        self.hardware_data = hw;
        // Pre-filter disk entries once here; render loop reads app.disk_data directly
        self.disk_data = self.hardware_data.iter()
            .filter(|r| r.component.contains("Disk Space"))
            .cloned()
            .collect();
    }

    pub fn apply_sys_update(&mut self, update: SysTickData) {
        if self.paused { return; }

        let mut procs = update.processes;
        self.sort_vec(&mut procs);
        self.usage_data = procs;

        // Rebuild filtered view first, then sync selection into it
        self.rebuild_filter();
        self.sync_selection();

        // CPU history
        self.cpu_history.push(update.cpu_pct as f64);
        if self.cpu_history.len() > 100 { self.cpu_history.remove(0); }

        // RAM from latest hardware scan (use raw_value to avoid string parse)
        let ram_pct = self.hardware_data.iter()
            .find(|r| r.component == "Physical RAM")
            .and_then(|r| r.raw_value)
            .unwrap_or(*self.ram_history.last().unwrap_or(&0.0));
        self.ram_history.push(ram_pct);
        if self.ram_history.len() > 100 { self.ram_history.remove(0); }

        // Rebuild chart vectors (per-250ms, not per-50ms frame)
        self.rebuild_chart_data();

        if self.recording {
            self.write_recording_row();
        }
    }

    // --- Internal helpers ---

    fn sort_vec(&self, procs: &mut Vec<ProcessData>) {
        if self.sort_by_cpu {
            procs.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage)
                .unwrap_or(std::cmp::Ordering::Equal));
        } else {
            procs.sort_by(|a, b| b.mem_mb.cmp(&a.mem_mb));
        }
    }

    fn rebuild_chart_data(&mut self) {
        let ram_start_x = 100.0 - self.ram_history.len() as f64;
        self.ram_chart_data = self.ram_history.iter().enumerate()
            .map(|(i, &y)| (ram_start_x + i as f64, y))
            .collect();

        let cpu_start_x = 100.0 - self.cpu_history.len() as f64;
        self.cpu_chart_data = self.cpu_history.iter().enumerate()
            .map(|(i, &y)| (cpu_start_x + i as f64, y))
            .collect();

        self.current_ram_pct = *self.ram_history.last().unwrap_or(&0.0);
        self.current_cpu_pct = *self.cpu_history.last().unwrap_or(&0.0);
    }

    pub fn rebuild_filter(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_usage = self.usage_data.clone();
        } else {
            let q = self.search_query.to_lowercase();
            self.filtered_usage = self.usage_data.iter()
                .filter(|p| p.name.to_lowercase().contains(&q)
                    || p.pid.to_string().contains(&q))
                .cloned()
                .collect();
        }
    }

    /// Re-map table selection to the correct index after usage_data or filter changes.
    /// Tracks by PID so sorting/refresh doesn't silently jump to a different process.
    pub fn sync_selection(&mut self) {
        match self.selected_pid {
            Some(pid) => {
                if let Some(idx) = self.filtered_usage.iter().position(|p| p.pid == pid) {
                    self.table_state.select(Some(idx));
                } else {
                    // Process exited or filtered out — reset
                    self.selected_pid = None;
                    self.kill_confirm_pid = None;
                    self.table_state.select(if self.filtered_usage.is_empty() { None } else { Some(0) });
                }
            }
            None => {
                if !self.filtered_usage.is_empty() && self.table_state.selected().is_none() {
                    self.table_state.select(Some(0));
                }
            }
        }
    }

    // --- Navigation ---

    pub fn select_up(&mut self) {
        let i = match self.table_state.selected() {
            Some(i) => if i == 0 { 0 } else { i - 1 },
            None => 0,
        };
        self.table_state.select(Some(i));
        self.selected_pid = self.filtered_usage.get(i).map(|p| p.pid);
        self.kill_confirm_pid = None;
    }

    pub fn select_down(&mut self) {
        let len = self.filtered_usage.len();
        let i = match self.table_state.selected() {
            Some(i) => if i >= len.saturating_sub(1) { i } else { i + 1 },
            None => 0,
        };
        self.table_state.select(Some(i));
        self.selected_pid = self.filtered_usage.get(i).map(|p| p.pid);
        self.kill_confirm_pid = None;
    }

    pub fn toggle_sort(&mut self) {
        self.sort_by_cpu = !self.sort_by_cpu;
        self.sort_vec(&mut self.usage_data.clone().into_iter().collect::<Vec<_>>().into_iter().collect::<Vec<_>>());
        // sort in place
        if self.sort_by_cpu {
            self.usage_data.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage)
                .unwrap_or(std::cmp::Ordering::Equal));
        } else {
            self.usage_data.sort_by(|a, b| b.mem_mb.cmp(&a.mem_mb));
        }
        self.rebuild_filter();
        self.sync_selection();
    }

    pub fn selected_process(&self) -> Option<&ProcessData> {
        self.table_state.selected().and_then(|i| self.filtered_usage.get(i))
    }

    // --- Kill confirmation (two-step: arm → execute) ---

    pub fn try_kill_selected(&mut self) {
        let Some(proc) = self.selected_process() else { return; };
        let pid = proc.pid;
        if self.kill_confirm_pid == Some(pid) {
            kill_process(pid);
            self.kill_confirm_pid = None;
        } else {
            self.kill_confirm_pid = Some(pid);
        }
    }

    /// Name of the process currently armed for kill confirmation, if any.
    pub fn kill_confirm_name(&self) -> Option<&str> {
        let pid = self.kill_confirm_pid?;
        self.filtered_usage.iter()
            .find(|p| p.pid == pid)
            .map(|p| p.name.as_str())
    }

    // --- Recording ---

    pub fn start_recording(&mut self) {
        use std::io::Write;
        match std::fs::OpenOptions::new()
            .write(true).create(true).truncate(true)
            .open(&self.export_path)
        {
            Ok(mut file) => {
                let _ = writeln!(file,
                    "Timestamp,CPU_Global%,RAM_Used%,Top_Process_Name,Top_Process_CPU%,Top_Process_MemMB");
                self.recording = true;
                self.last_export_error = None;
            }
            Err(e) => {
                self.last_export_error = Some(format!("Cannot create file: {}", e));
            }
        }
    }

    fn write_recording_row(&mut self) {
        use std::io::Write;
        match std::fs::OpenOptions::new().append(true).open(&self.export_path) {
            Ok(mut file) => {
                let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                let top = self.usage_data.first();
                let _ = writeln!(file, "{},{:.2},{:.2},{},{:.2},{}",
                    ts,
                    self.current_cpu_pct,
                    self.current_ram_pct,
                    top.map(|p| p.name.as_str()).unwrap_or("None"),
                    top.map(|p| p.cpu_usage).unwrap_or(0.0),
                    top.map(|p| p.mem_mb).unwrap_or(0),
                );
            }
            Err(e) => {
                log::warn!("Recording write failed: {}", e);
                self.recording = false;
                self.last_export_error = Some(format!("Record write failed: {}", e));
            }
        }
    }

    /// Absolute path shown in the export pane so user knows where the file lands.
    pub fn export_resolved_path(&self) -> String {
        let p = std::path::Path::new(&self.export_path);
        if p.is_absolute() {
            self.export_path.clone()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(p).display().to_string())
                .unwrap_or_else(|_| self.export_path.clone())
        }
    }
}

// --- Windows process termination (avoids sysinfo System alloc just to kill) ---

fn kill_process(pid: u32) {
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    use windows::Win32::Foundation::{CloseHandle, BOOL};
    unsafe {
        match OpenProcess(PROCESS_TERMINATE, BOOL(0), pid) {
            Ok(handle) => {
                if let Err(e) = TerminateProcess(handle, 1) {
                    log::warn!("TerminateProcess({}): {:?}", pid, e);
                }
                let _ = CloseHandle(handle);
            }
            Err(e) => log::warn!("OpenProcess({}): {:?}", pid, e),
        }
    }
}
