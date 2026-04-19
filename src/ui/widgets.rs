use ratatui::{prelude::*, widgets::*};
use crate::models::{ScanResult, ProcessData};
use crate::app::ActivePane;

const PRIMARY: Color = Color::Rgb(97, 207, 90);
const NEUTRAL: Color = Color::White;
const INACTIVE_BORDER: Color = Color::DarkGray;

/// Disk gauge — uses pre-parsed raw_value to avoid string parsing on every frame.
pub fn draw_disk_gauge(f: &mut Frame, area: Rect, result: &ScanResult, active: bool) {
    let val = result.raw_value.unwrap_or_else(|| {
        result.value.replace("% Full", "").trim().parse::<f64>().unwrap_or(0.0)
    });

    let border_color = if active { PRIMARY } else { INACTIVE_BORDER };

    let gauge = Gauge::default()
        .block(Block::default()
            .title(format!(" Disk: {} ", result.component))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .border_type(BorderType::Thick))
        .gauge_style(Style::default().fg(PRIMARY).bg(Color::Black))
        .percent(val as u16)
        .label(Span::styled(
            format!("{:.1}%", val),
            Style::default().fg(NEUTRAL).add_modifier(Modifier::BOLD),
        ));

    f.render_widget(gauge, area);
}

/// Chart wave — accepts pre-computed (x, y) data computed in App::rebuild_chart_data().
/// Avoids allocating two Vecs every 50ms repaint; those Vecs are rebuilt only on 250ms tick.
pub fn draw_ram_wave(
    f: &mut Frame,
    area: Rect,
    ram_data: &[(f64, f64)],
    cpu_data: &[(f64, f64)],
    current_ram: f64,
    current_cpu: f64,
    active: bool,
) {
    let datasets = vec![
        Dataset::default()
            .name(format!(" RAM: {:.1}% ", current_ram))
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(PRIMARY))
            .data(ram_data),
        Dataset::default()
            .name(format!(" CPU: {:.1}% ", current_cpu))
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Gray))
            .data(cpu_data),
    ];

    let border_color = if active { PRIMARY } else { INACTIVE_BORDER };

    let chart = Chart::new(datasets)
        .block(Block::default()
            .title(" Utilization History (%) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .border_type(BorderType::Thick))
        .x_axis(Axis::default()
            .title(Span::styled("Time", Style::default().fg(Color::Gray)))
            .bounds([0.0, 100.0])
            .labels(vec![
                Span::styled("100s ago", Style::default().fg(Color::Gray)),
                Span::styled("50s ago", Style::default().fg(Color::Gray)),
                Span::styled("Now", Style::default().fg(Color::Gray)),
            ]))
        .y_axis(Axis::default()
            .title(Span::styled("Usage", Style::default().fg(Color::Gray)))
            .bounds([0.0, 100.0])
            .labels(vec![
                Span::styled("0%", Style::default().fg(Color::Gray)),
                Span::styled("50%", Style::default().fg(Color::Gray)),
                Span::styled("100%", Style::default().fg(Color::Gray)),
            ]));

    f.render_widget(chart, area);
}

pub fn draw_power_stats(f: &mut Frame, area: Rect, hardware: &[ScanResult], active: bool) {
    let items: Vec<ListItem> = hardware.iter()
        .filter(|s| !s.component.contains("Disk Space") && !s.component.contains("Physical RAM"))
        .map(|stat| ListItem::new(Line::from(vec![
            Span::styled(format!("{}: ", stat.component),
                Style::default().fg(NEUTRAL).add_modifier(Modifier::BOLD)),
            Span::styled(&stat.value, Style::default().fg(PRIMARY)),
        ])))
        .collect();

    let border_color = if active { PRIMARY } else { INACTIVE_BORDER };

    let list = List::new(items)
        .block(Block::default()
            .title(" Hardware Stats ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .border_type(BorderType::Thick));

    f.render_widget(list, area);
}

/// Header — shows REC and/or PAUSED indicators alongside app name and host info.
pub fn draw_header(f: &mut Frame, area: Rect, host_name: &str, recording: bool, paused: bool) {
    let header_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let mut left_spans: Vec<Span> = Vec::new();
    if recording {
        left_spans.push(Span::styled(" [ REC ] ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD).add_modifier(Modifier::SLOW_BLINK)));
    }
    if paused {
        left_spans.push(Span::styled(" [ PAUSED ] ",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
    }
    left_spans.push(Span::styled(" Velox", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)));
    left_spans.push(Span::styled(" (v1.0) ", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)));

    let left = Paragraph::new(Line::from(left_spans))
        .alignment(Alignment::Left)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(Style::default().fg(PRIMARY)));

    let right = Paragraph::new(Span::styled(
        format!("{} ", host_name),
        Style::default().fg(NEUTRAL).add_modifier(Modifier::BOLD),
    ))
    .alignment(Alignment::Right)
    .block(Block::default()
        .borders(Borders::TOP | Borders::BOTTOM | Borders::RIGHT)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(PRIMARY)));

    f.render_widget(left, header_layout[0]);
    f.render_widget(right, header_layout[1]);
}

/// Export pane — shows resolved absolute path so user knows where the file will land.
/// Shows error in red if last export/record operation failed.
pub fn draw_export_pane(
    f: &mut Frame,
    area: Rect,
    export_path: &str,
    resolved_path: &str,
    active: bool,
    typing: bool,
    recording: bool,
    error: Option<&str>,
) {
    let border_color = if typing { Color::Yellow } else if active { PRIMARY } else { INACTIVE_BORDER };
    let prompt_color = if typing { Color::Yellow } else { NEUTRAL };

    let mut text = vec![
        Line::from(vec![
            Span::styled(" Mode: ", Style::default().add_modifier(Modifier::BOLD).fg(PRIMARY)),
            if recording {
                Span::styled("● Recording", Style::default().fg(Color::Red))
            } else {
                Span::styled("Snapshot", Style::default().fg(Color::Gray))
            },
        ]),
        Line::from(vec![
            Span::styled(" Path: ", Style::default().add_modifier(Modifier::BOLD).fg(PRIMARY)),
            Span::styled(export_path, Style::default().fg(prompt_color)),
            if typing { Span::styled("█", Style::default().fg(Color::Yellow)) } else { Span::raw("") },
        ]),
        Line::from(vec![
            Span::styled("   → ", Style::default().fg(Color::DarkGray)),
            Span::styled(resolved_path, Style::default().fg(Color::DarkGray)),
        ]),
    ];

    if let Some(err) = error {
        text.push(Line::from(Span::styled(
            format!(" ✗ {}", err),
            Style::default().fg(Color::Red),
        )));
    }

    let p = Paragraph::new(text)
        .block(Block::default()
            .title(" Record Telemetry ")
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(Style::default().fg(border_color)));

    f.render_widget(p, area);
}

/// Footer — shows context-sensitive key hints.
/// Kill confirmation overrides the normal hint to prompt the user.
/// Pause state shown when paused.
pub fn draw_footer(
    f: &mut Frame,
    area: Rect,
    active_pane: &ActivePane,
    zoomed: bool,
    input_mode: bool,
    kill_confirm_pid: Option<u32>,
    kill_confirm_name: Option<&str>,
    paused: bool,
) {
    let (text, color) = if let (Some(pid), Some(name)) = (kill_confirm_pid, kill_confirm_name) {
        (
            format!("CONFIRM KILL: {} (PID {}) — press x again to kill | any other key to cancel", name, pid),
            Color::Red,
        )
    } else if input_mode {
        ("Esc: Cancel | Enter: Submit".to_string(), Color::Gray)
    } else if paused {
        ("PAUSED — p: Resume | q: Quit | Tab: Cycle | r: Toggle Record".to_string(), Color::Yellow)
    } else if zoomed {
        let instr = match active_pane {
            ActivePane::Processes => "↑↓: Scroll | i: Search | s: Sort | x: Kill | p: Pause",
            ActivePane::Export    => "i: Edit Path | r: Toggle Record",
            _                     => "Monitoring",
        };
        (format!("Esc/q: Back | {}", instr), Color::Gray)
    } else {
        ("Tab: Cycle | Enter: Zoom | r: Record | p: Pause | q: Quit".to_string(), Color::Gray)
    };

    let p = Paragraph::new(Span::styled(text,
        Style::default().fg(color).add_modifier(Modifier::BOLD)))
        .alignment(Alignment::Center);
    f.render_widget(p, area);
}

pub fn draw_splash(f: &mut Frame, area: Rect, elapsed_ms: u64) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(10),
            Constraint::Length(3),
        ])
        .split(area);

    let boot_steps = [
        "VERIFYING_KERNEL_INTEGRITY...",
        "CONNECTING_PROBES_TO_HAL...",
        "INITIALIZING_WMI_DATABASE...",
        "SCANNING_HARDWARE_TOPOLOGY...",
        "ESTABLISHING_TELEMETRY_STREAM...",
        "LOADING_PANE_VISUALIZERS...",
        "OPTIMIZING_BRAILLE_BUFFERS...",
        "BOOT_SUCCESSFUL_STARTING_DASHBOARD",
    ];

    let current_step = (elapsed_ms / 375).min(7) as usize;
    let start_idx = if current_step > 4 { current_step - 4 } else { 0 };
    let boot_lines: Vec<Line> = (start_idx..=current_step).map(|i| {
        let text = if i == current_step && elapsed_ms % 500 < 250 {
            format!("> {} █", boot_steps[i])
        } else {
            format!("[ OK ] {}", boot_steps[i])
        };
        Line::from(Span::styled(text,
            Style::default().fg(if i == 7 { PRIMARY } else { Color::Gray })))
    }).collect();

    f.render_widget(Paragraph::new(boot_lines).alignment(Alignment::Center), chunks[1]);

    let progress = (elapsed_ms as f64 / 3000.0 * 100.0).min(100.0);
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(PRIMARY)))
        .gauge_style(Style::default().fg(PRIMARY).bg(Color::Black))
        .percent(progress as u16)
        .label(format!(" VELOX INITIALIZING... {:.0}% ", progress));
    f.render_widget(gauge, chunks[2]);
}

pub fn draw_process_table(
    f: &mut Frame,
    area: Rect,
    usage: &[ProcessData],
    state: &mut TableState,
    active: bool,
    typing: bool,
    search_query: &str,
) {
    let (table_area, search_area) = if typing || !search_query.is_empty() {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(5)])
            .split(area);
        (layout[1], Some(layout[0]))
    } else {
        (area, None)
    };

    if let Some(s_area) = search_area {
        let border_color = if typing { Color::Yellow } else { PRIMARY };
        let p = Paragraph::new(Line::from(vec![
            Span::styled(" Query: ", Style::default().fg(PRIMARY)),
            Span::styled(search_query,
                Style::default().fg(if typing { Color::Yellow } else { NEUTRAL })),
            if typing { Span::styled("█", Style::default().fg(Color::Yellow)) } else { Span::raw("") },
        ]))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(Style::default().fg(border_color)));
        f.render_widget(p, s_area);
    }

    let header = Row::new(vec![" PID", "PROC", "CPU%", "MEM (MB)"])
        .style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD))
        .bottom_margin(1);

    let rows = usage.iter().map(|r| {
        Row::new(vec![
            format!(" {}", r.pid),
            r.name.clone(),
            format!(" {:.1}%", r.cpu_usage),
            format!(" {}", r.mem_mb),
        ])
        .style(Style::default().fg(NEUTRAL))
    });

    let widths = [
        Constraint::Percentage(15),
        Constraint::Percentage(45),
        Constraint::Percentage(20),
        Constraint::Percentage(20),
    ];

    let border_color = if active && !typing { PRIMARY } else { INACTIVE_BORDER };

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default()
            .title(" Process Behavior ")
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(Style::default().fg(border_color)))
        .highlight_style(Style::default().bg(PRIMARY).fg(Color::Black).add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");

    f.render_stateful_widget(table, table_area, state);
}
