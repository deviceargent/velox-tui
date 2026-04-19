mod app;
mod models;
mod scanner;
mod ui;

use app::{App, Screen, ActivePane};
use scanner::worker::ScanWorker;
use scanner::Scannable;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{error::Error, io, time::Duration};

fn main() -> Result<(), Box<dyn Error>> {
    // Fix #19: set a default log level so warnings appear without RUST_LOG env var
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Warn)
        .init();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Fix #5: all scanners registered at init, not ad-hoc
    let sw_scanner = scanner::software::SoftwareScanner::new();
    let worker = ScanWorker::spawn();

    // One-time sysinfo read for static header info
    let mut sys = sysinfo::System::new();
    sys.refresh_cpu();
    let os_name = sysinfo::System::name().unwrap_or_else(|| "Windows".into());
    let os_ver = sysinfo::System::os_version().unwrap_or_else(|| "10".into());
    let cpu_brand = sys.cpus().first()
        .map(|c| c.brand().to_string())
        .unwrap_or_else(|| "Unknown CPU".into());
    let host_name = format!("{} {} | {}", os_name, os_ver, cpu_brand);

    let app = App::new(host_name);
    let res = run_app(&mut terminal, worker, sw_scanner, app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }
    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    worker: ScanWorker,
    mut sw_scanner: scanner::software::SoftwareScanner,
    mut app: App,
) -> io::Result<()> {
    let ui_tick = Duration::from_millis(50);

    loop {
        // Fix #1/#2: drain channels non-blocking; App updates caches only when data changes
        while let Ok(hw) = worker.hw_rx.try_recv() {
            app.apply_hw_update(hw);
        }
        // Fix #14: skip sys drain when paused — freezes process table for reading
        if !app.paused {
            while let Ok(update) = worker.sys_rx.try_recv() {
                app.apply_sys_update(update);
            }
        }

        // Splash timeout
        if let Screen::Splash(start) = app.screen {
            if start.elapsed().as_millis() >= 3000 {
                app.screen = Screen::Dashboard;
            }
        }

        terminal.draw(|f| ui::render(f, &mut app))?;

        if app.should_quit { break; }

        if crossterm::event::poll(ui_tick)? {
            if let Event::Key(key) = crossterm::event::read()? {
                if key.kind == KeyEventKind::Press {
                    // Skip splash on keypress
                    if app.screen != Screen::Dashboard {
                        app.screen = Screen::Dashboard;
                        continue;
                    }
                    handle_key(&mut app, key.code, &mut sw_scanner);
                }
            }
        }
    }

    Ok(())
}

fn handle_key(
    app: &mut App,
    code: KeyCode,
    sw_scanner: &mut scanner::software::SoftwareScanner,
) {
    // Any key besides 'x' while kill confirm is armed cancels it
    if app.kill_confirm_pid.is_some() {
        if code != KeyCode::Char('x') && code != KeyCode::Delete {
            app.kill_confirm_pid = None;
            // Don't return — still process the key
        }
    }

    if app.input_mode {
        handle_input_key(app, code, sw_scanner);
    } else {
        handle_normal_key(app, code);
    }
}

fn handle_input_key(
    app: &mut App,
    code: KeyCode,
    sw_scanner: &mut scanner::software::SoftwareScanner,
) {
    match code {
        KeyCode::Char(c) => {
            if app.active_pane == ActivePane::Processes {
                app.search_query.push(c);
                app.rebuild_filter();
                app.sync_selection();
            } else if app.active_pane == ActivePane::Export {
                app.export_path.push(c);
            }
        }
        KeyCode::Backspace => {
            if app.active_pane == ActivePane::Processes {
                app.search_query.pop();
                app.rebuild_filter();
                app.sync_selection();
            } else if app.active_pane == ActivePane::Export {
                app.export_path.pop();
            }
        }
        KeyCode::Esc => {
            app.input_mode = false;
        }
        KeyCode::Enter => {
            if app.active_pane == ActivePane::Export {
                do_export(app, sw_scanner);
                app.input_mode = false;
            }
        }
        _ => {}
    }
}

fn handle_normal_key(app: &mut App, code: KeyCode) {
    let num_disks = app.disk_data.len();

    match code {
        KeyCode::Char('i') => {
            if app.active_pane == ActivePane::Processes || app.active_pane == ActivePane::Export {
                app.input_mode = true;
            }
        }
        KeyCode::Char('r') => {
            if app.recording {
                app.recording = false;
            } else {
                app.start_recording();
            }
        }
        // Fix #14: 'p' toggles pause
        KeyCode::Char('p') => {
            app.paused = !app.paused;
        }
        KeyCode::Char('q') => {
            if app.zoomed {
                app.zoomed = false;
            } else {
                app.should_quit = true;
            }
        }
        KeyCode::Esc | KeyCode::Backspace => {
            if app.zoomed {
                app.zoomed = false;
            } else if code == KeyCode::Esc {
                app.should_quit = true;
            }
        }
        KeyCode::Enter => {
            if app.active_pane == ActivePane::Export {
                app.input_mode = true;
            } else {
                app.zoomed = true;
            }
        }
        KeyCode::Tab => {
            if !app.zoomed {
                app.active_pane = match app.active_pane {
                    ActivePane::Disk(i) => {
                        if i + 1 < num_disks { ActivePane::Disk(i + 1) } else { ActivePane::Ram }
                    }
                    ActivePane::Ram      => ActivePane::Power,
                    ActivePane::Power    => ActivePane::Export,
                    ActivePane::Export   => ActivePane::Processes,
                    ActivePane::Processes => {
                        if num_disks > 0 { ActivePane::Disk(0) } else { ActivePane::Ram }
                    }
                };
                // Tab movement cancels kill confirmation
                app.kill_confirm_pid = None;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.active_pane == ActivePane::Processes || app.zoomed {
                app.select_down();
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.active_pane == ActivePane::Processes || app.zoomed {
                app.select_up();
            }
        }
        KeyCode::Char('s') => {
            app.toggle_sort();
        }
        // Fix #11: two-step kill confirmation
        KeyCode::Char('x') | KeyCode::Delete => {
            if app.active_pane == ActivePane::Processes || app.zoomed {
                app.try_kill_selected();
            }
        }
        _ => {}
    }
}

fn do_export(app: &mut App, sw_scanner: &mut scanner::software::SoftwareScanner) {
    use std::io::Write;
    use std::collections::BTreeMap;

    match std::fs::File::create(&app.export_path) {
        Ok(mut file) => {
            let _ = writeln!(file, "=== Velox Diagnostica Report ===");
            let _ = writeln!(file, "System: {}", app.host_name);
            let _ = writeln!(file, "RAM: {:.2}%", app.current_ram_pct);
            let _ = writeln!(file, "CPU: {:.2}%\n", app.current_cpu_pct);

            let mut all_scans = app.hardware_data.clone();
            all_scans.extend(sw_scanner.scan());

            let mut categorized: BTreeMap<String, Vec<&models::ScanResult>> = BTreeMap::new();
            for scan in &all_scans {
                categorized.entry(scan.category.clone()).or_default().push(scan);
            }
            for (cat, items) in categorized {
                let _ = writeln!(file, "[{}]", cat);
                for item in items {
                    let _ = writeln!(file, "  {}: {} ({}) [Endpoint: {}]",
                        item.component, item.value, item.description, item.endpoint);
                }
                let _ = writeln!(file, "");
            }

            let _ = writeln!(file, "[Usage Telemetry]");
            let mut top = app.usage_data.clone();
            top.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage)
                .unwrap_or(std::cmp::Ordering::Equal));
            for proc in top.iter().take(25) {
                let _ = writeln!(file,
                    "  PID {}: {} - CPU: {:.1}% | MEM: {}MB [Endpoint: sysinfo::System (NtQuerySystemInformation)]",
                    proc.pid, proc.name, proc.cpu_usage, proc.mem_mb);
            }

            app.last_export_error = None;
        }
        Err(e) => {
            log::warn!("Export failed: {}", e);
            app.last_export_error = Some(format!("Export failed: {}", e));
        }
    }
}
