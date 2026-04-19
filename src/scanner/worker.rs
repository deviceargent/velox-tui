use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};
use wmi::{WMIConnection, COMLibrary};
use crate::models::{ScanResult, SysTickData};
use crate::scanner::{Scannable, hardware::HardwareScanner, usage::UsageScanner};

/// Owns the background scanner thread and exposes receive-only channels for UI data.
pub struct ScanWorker {
    pub hw_rx: Receiver<Vec<ScanResult>>,
    pub sys_rx: Receiver<SysTickData>,
}

impl ScanWorker {
    pub fn spawn() -> Self {
        let (hw_tx, hw_rx) = mpsc::channel();
        let (sys_tx, sys_rx) = mpsc::channel();

        thread::spawn(move || {
            // --- Issue #2: COM + WMI initialized ONCE for the lifetime of this thread ---
            let wmi_con = COMLibrary::new()
                .map_err(|e| { log::error!("COM init failed: {:?}", e); e })
                .ok()
                .and_then(|lib| {
                    WMIConnection::new(lib.into())
                        .map_err(|e| { log::error!("WMI connection failed: {:?}", e); e })
                        .ok()
                });

            // --- Issue #3: HardwareScanner owns its System; only one System per scanner ---
            let mut hw_scanner = HardwareScanner::new(wmi_con);
            // --- Issue #5: All scanners created at init, not ad-hoc ---
            let mut us_scanner = UsageScanner::new();

            let hw_rate = Duration::from_secs(5);
            let sys_rate = Duration::from_millis(250);
            let mut last_hw = Instant::now() - hw_rate; // force first hw scan immediately

            loop {
                thread::sleep(sys_rate);

                // Sys tick: process list + global CPU % (refresh_all already done inside)
                let processes = us_scanner.scan_process_behavior();
                let cpu_pct = us_scanner.last_cpu_pct();
                if sys_tx.send(SysTickData { processes, cpu_pct }).is_err() {
                    break; // UI dropped receiver — shut down
                }

                // HW tick every 5 s
                if last_hw.elapsed() >= hw_rate {
                    let hw_data = hw_scanner.scan();
                    if hw_tx.send(hw_data).is_err() {
                        break;
                    }
                    last_hw = Instant::now();
                }
            }
        });

        Self { hw_rx, sys_rx }
    }
}
