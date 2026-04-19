use crate::models::{ScanResult, Severity};
use crate::scanner::Scannable;
use sysinfo::{System, Disks};
use std::collections::HashMap;
use wmi::WMIConnection;
use std::process::Command;

pub struct HardwareScanner {
    wmi_con: Option<WMIConnection>,
    sys: System,
}

impl HardwareScanner {
    pub fn new(wmi_con: Option<WMIConnection>) -> Self {
        let sys = System::new();
        Self { wmi_con, sys }
    }

    pub fn scan_disk_health(&self) -> Vec<ScanResult> {
        let mut results = Vec::new();
        let disks = Disks::new_with_refreshed_list();

        for disk in &disks {
            let name = disk.name().to_string_lossy();
            let total_gb = disk.total_space() / 1024 / 1024 / 1024;
            let available_gb = disk.available_space() / 1024 / 1024 / 1024;

            if total_gb == 0 { continue; }
            let usage_pct = 100.0 - (available_gb as f64 / total_gb as f64 * 100.0);

            let severity = if usage_pct > 90.0 { Severity::High }
                else if usage_pct > 75.0 { Severity::Medium }
                else { Severity::Low };

            results.push(ScanResult {
                category: "Hardware Limitations".into(),
                component: format!("Disk Space ({})", name),
                value: format!("{:.1}% Full", usage_pct),
                raw_value: Some(usage_pct),
                severity,
                description: format!("{} GB free of {} GB.", available_gb, total_gb),
                endpoint: "sysinfo::Disks".into(),
                pid: None,
            });
        }
        results
    }

    pub fn scan_memory_pressure(&mut self) -> Vec<ScanResult> {
        let mut results = Vec::new();
        self.sys.refresh_memory();

        let total_ram = self.sys.total_memory();
        let used_ram = self.sys.used_memory();
        let total_swap = self.sys.total_swap();
        let used_swap = self.sys.used_swap();

        let ram_usage_pct = (used_ram as f64 / total_ram as f64) * 100.0;

        results.push(ScanResult {
            category: "Hardware Limitations".into(),
            component: "Physical RAM".into(),
            value: format!("{:.1}% Used", ram_usage_pct),
            raw_value: Some(ram_usage_pct),
            severity: if ram_usage_pct > 90.0 { Severity::High } else { Severity::Low },
            description: format!("Using {}MB / {}MB", used_ram / 1024 / 1024, total_ram / 1024 / 1024),
            endpoint: "sysinfo::System".into(),
            pid: None,
        });

        if total_swap > 0 {
            let swap_usage_pct = (used_swap as f64 / total_swap as f64) * 100.0;
            if swap_usage_pct > 50.0 || ram_usage_pct > 95.0 {
                results.push(ScanResult {
                    category: "Hardware Limitations".into(),
                    component: "Memory Pressure".into(),
                    value: "High (Thrashing)".into(),
                    raw_value: Some(swap_usage_pct),
                    severity: Severity::Critical,
                    description: "System is over-reliant on Pagefile.".into(),
                    endpoint: "sysinfo::System".into(),
                    pid: None,
                });
            }
        }
        results
    }

    pub fn scan_cpu_thermals(&self) -> Vec<ScanResult> {
        let mut results = Vec::new();
        let wmi_con = match &self.wmi_con {
            Some(c) => c,
            None => {
                log::warn!("scan_cpu_thermals: WMI unavailable, skipping");
                return results;
            }
        };

        let rows: Vec<HashMap<String, serde_json::Value>> = wmi_con
            .raw_query("SELECT CurrentClockSpeed, MaxClockSpeed, LoadPercentage FROM Win32_Processor")
            .unwrap_or_else(|e| { log::warn!("WMI Win32_Processor: {:?}", e); vec![] });

        for cpu in rows {
            let current = cpu.get("CurrentClockSpeed").and_then(|v| v.as_u64()).unwrap_or(0);
            let max = cpu.get("MaxClockSpeed").and_then(|v| v.as_u64()).unwrap_or(1);
            let load = cpu.get("LoadPercentage").and_then(|v| v.as_u64()).unwrap_or(0);
            let throttle_ratio = (current as f64 / max as f64) * 100.0;

            let status_value = format!("{}MHz / {}MHz ({:.0}%)", current, max, throttle_ratio);
            let throttling = load > 80 && throttle_ratio < 60.0;

            results.push(ScanResult {
                category: "Hardware Limitations".into(),
                component: "CPU Thermal State".into(),
                value: status_value,
                raw_value: Some(throttle_ratio),
                severity: if throttling { Severity::Critical } else { Severity::Low },
                description: if throttling {
                    format!("Throttling detected! Load is {}% but speed is capped.", load)
                } else {
                    format!("CPU Load: {}%. Thermals appear stable.", load)
                },
                endpoint: "WMI::Win32_Processor".into(),
                pid: None,
            });
        }
        results
    }

    pub fn scan_gpu_and_power(&self) -> Vec<ScanResult> {
        let mut results = Vec::new();
        let wmi_con = match &self.wmi_con {
            Some(c) => c,
            None => {
                log::warn!("scan_gpu_and_power: WMI unavailable, skipping");
                return results;
            }
        };

        let gpu_rows: Vec<HashMap<String, serde_json::Value>> = wmi_con
            .raw_query("SELECT Name, AdapterRAM FROM Win32_VideoController")
            .unwrap_or_else(|e| { log::warn!("WMI Win32_VideoController: {:?}", e); vec![] });

        for gpu in gpu_rows {
            let name = gpu.get("Name").and_then(|v| v.as_str()).unwrap_or("Unknown GPU");
            let ram_bytes = gpu.get("AdapterRAM").and_then(|v| v.as_u64()).unwrap_or(0);
            let ram_gb = (ram_bytes as f64 / 1024.0 / 1024.0 / 1024.0).round() as u64;

            results.push(ScanResult {
                category: "Hardware Limitations".into(),
                component: "GPU VRAM".into(),
                value: format!("{} GB", ram_gb),
                raw_value: Some(ram_gb as f64),
                severity: if ram_gb < 4 { Severity::Medium } else { Severity::Low },
                description: format!("Model: {}. Low VRAM causes UI stutter in modern Windows.", name),
                endpoint: "WMI::Win32_VideoController".into(),
                pid: None,
            });
        }

        let bat_rows: Vec<HashMap<String, serde_json::Value>> = wmi_con
            .raw_query("SELECT BatteryStatus, DesignVoltage FROM Win32_Battery")
            .unwrap_or_else(|e| { log::warn!("WMI Win32_Battery: {:?}", e); vec![] });

        if let Some(bat) = bat_rows.first() {
            let status = bat.get("BatteryStatus").and_then(|v| v.as_u64()).unwrap_or(2);
            let voltage = bat.get("DesignVoltage").and_then(|v| v.as_u64()).unwrap_or(0);
            let on_battery = status != 2 && status != 3;

            results.push(ScanResult {
                category: "Hardware Limitations".into(),
                component: "Power Source".into(),
                value: format!("{} ({}mV)", if on_battery { "Battery" } else { "AC Power" }, voltage),
                raw_value: Some(if on_battery { 0.0 } else { 1.0 }),
                severity: if on_battery { Severity::Medium } else { Severity::Low },
                description: if on_battery {
                    "Battery voltage may limit peak clock speeds."
                } else {
                    "Clean power delivery detected."
                }.into(),
                endpoint: "WMI::Win32_Battery".into(),
                pid: None,
            });
        }

        match Command::new("powercfg")
            .args(&["/query", "SCHEME_CURRENT", "SUB_PROCESSOR", "PROCTHROTTLEMAX"])
            .output()
        {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout);
                if let Some(pos) = text.find("Current AC Power Setting Index: ") {
                    let hex_str = text[pos + 32..].split_whitespace().next().unwrap_or("0x64");
                    let cap = u64::from_str_radix(hex_str.trim_start_matches("0x"), 16)
                        .unwrap_or(100);

                    results.push(ScanResult {
                        category: "Hardware Limitations".into(),
                        component: "CPU Performance Cap".into(),
                        value: format!("{}%", cap),
                        raw_value: Some(cap as f64),
                        severity: if cap < 100 { Severity::High } else { Severity::Low },
                        description: format!("Windows Power Scheme capping CPU to {}%.", cap),
                        endpoint: "Process::powercfg".into(),
                        pid: None,
                    });
                }
            }
            Err(e) => log::warn!("powercfg query failed: {:?}", e),
        }

        results
    }
}

impl Scannable for HardwareScanner {
    fn scan(&mut self) -> Vec<ScanResult> {
        let mut all = self.scan_disk_health();
        all.extend(self.scan_memory_pressure());
        all.extend(self.scan_cpu_thermals());
        all.extend(self.scan_gpu_and_power());
        all
    }
}
