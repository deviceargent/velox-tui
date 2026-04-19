use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub fn create_main_layout(area: Rect) -> Vec<Rect> {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Content
            Constraint::Length(1), // Footer
        ])
        .split(area);
    main_layout.to_vec()
}

pub fn create_layout(content_area: Rect, disk_count: usize) -> Vec<Rect> {
    let content_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40), // Left: Disks & RAM & Power & Export
            Constraint::Percentage(60), // Right: Process Table
        ])
        .split(content_area);

    let mut left_constraints = vec![Constraint::Length(3); disk_count];
    left_constraints.push(Constraint::Min(6)); // RAM Graph
    left_constraints.push(Constraint::Length(6)); // Power Stats
    left_constraints.push(Constraint::Length(3)); // Export Pane

    let left_column = Layout::default()
        .direction(Direction::Vertical)
        .constraints(left_constraints)
        .split(content_split[0]);

    // Flattening: [Disk1, Disk2... DiskN, RAM_Graph, Power_Stats, Export_Pane, Process_Table]
    let mut areas = left_column.to_vec();
    areas.push(content_split[1]);
    areas
}