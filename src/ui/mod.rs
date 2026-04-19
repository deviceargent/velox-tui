pub mod layout;
pub mod widgets;

use ratatui::Frame;
use crate::app::{App, ActivePane, Screen};

pub fn render(f: &mut Frame, app: &mut App) {
    let main_chunks = layout::create_main_layout(f.size());

    widgets::draw_header(f, main_chunks[0], &app.host_name, app.recording, app.paused);

    if let Screen::Splash(start) = app.screen {
        let elapsed = start.elapsed().as_millis() as u64;
        widgets::draw_splash(f, main_chunks[1], elapsed);
        widgets::draw_footer(f, main_chunks[2], &app.active_pane, app.zoomed,
            app.input_mode, app.kill_confirm_pid, app.kill_confirm_name(), app.paused);
        return;
    }

    let resolved = app.export_resolved_path();

    if app.zoomed {
        let content = main_chunks[1];
        match app.active_pane {
            ActivePane::Disk(idx) => {
                if let Some(disk) = app.disk_data.get(idx).cloned() {
                    widgets::draw_disk_gauge(f, content, &disk, true);
                }
            }
            ActivePane::Ram => {
                widgets::draw_ram_wave(f, content,
                    &app.ram_chart_data, &app.cpu_chart_data,
                    app.current_ram_pct, app.current_cpu_pct, true);
            }
            ActivePane::Power => {
                widgets::draw_power_stats(f, content, &app.hardware_data, true);
            }
            ActivePane::Export => {
                widgets::draw_export_pane(f, content, &app.export_path, &resolved,
                    true, app.input_mode, app.recording,
                    app.last_export_error.as_deref());
            }
            ActivePane::Processes => {
                widgets::draw_process_table(f, content, &app.filtered_usage,
                    &mut app.table_state, true, app.input_mode, &app.search_query);
            }
        }
    } else {
        let ndisks = app.disk_data.len();
        let chunks = layout::create_layout(main_chunks[1], ndisks);

        for i in 0..ndisks {
            if let Some(disk) = app.disk_data.get(i).cloned() {
                widgets::draw_disk_gauge(f, chunks[i], &disk,
                    app.active_pane == ActivePane::Disk(i));
            }
        }

        widgets::draw_ram_wave(f, chunks[ndisks],
            &app.ram_chart_data, &app.cpu_chart_data,
            app.current_ram_pct, app.current_cpu_pct,
            app.active_pane == ActivePane::Ram);

        widgets::draw_power_stats(f, chunks[ndisks + 1], &app.hardware_data,
            app.active_pane == ActivePane::Power);

        let export_typing = app.input_mode && app.active_pane == ActivePane::Export;
        widgets::draw_export_pane(f, chunks[ndisks + 2], &app.export_path, &resolved,
            app.active_pane == ActivePane::Export, export_typing,
            app.recording, app.last_export_error.as_deref());

        let proc_typing = app.input_mode && app.active_pane == ActivePane::Processes;
        widgets::draw_process_table(f, chunks[ndisks + 3], &app.filtered_usage,
            &mut app.table_state, app.active_pane == ActivePane::Processes,
            proc_typing, &app.search_query);
    }

    widgets::draw_footer(f, main_chunks[2], &app.active_pane, app.zoomed,
        app.input_mode, app.kill_confirm_pid, app.kill_confirm_name(), app.paused);
}
