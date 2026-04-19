use crate::models::ProcessData;
use sysinfo::System;

pub struct UsageScanner {
    sys: System,
}

impl UsageScanner {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self { sys }
    }

    pub fn scan_process_behavior(&mut self) -> Vec<ProcessData> {
        let mut results = Vec::new();
        self.sys.refresh_all();

        let core_count = self.sys.cpus().len() as f32;

        for (pid, process) in self.sys.processes() {
            let name = process.name();

            let total_cpu_impact = process.cpu_usage() / core_count;
            let memory_mb = process.memory() / 1024 / 1024;

            if memory_mb > 0 || total_cpu_impact > 0.0 {
                results.push(ProcessData {
                    pid: pid.as_u32(),
                    name: name.to_string(),
                    cpu_usage: total_cpu_impact,
                    mem_mb: memory_mb,
                });
            }
        }

        results.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(150);
        results
    }

    /// Returns global CPU usage from the last refresh (call after scan_process_behavior).
    pub fn last_cpu_pct(&self) -> f32 {
        self.sys.global_cpu_info().cpu_usage()
    }
}
