use crate::models::{ScanResult, Severity};
use crate::scanner::Scannable;
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumValueW, RegOpenKeyExW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
    KEY_READ, HKEY
};
use windows::core::{PCWSTR, PWSTR};
use sysinfo::System;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_Status,
    CM_Locate_DevNodeW,
    CM_LOCATE_DEVNODE_NORMAL,
    CR_SUCCESS,
    CM_DEVNODE_STATUS_FLAGS,
    CM_PROB,
};

pub struct SoftwareScanner;

impl SoftwareScanner {
    pub fn new() -> Self { Self }

    pub fn scan_startup_registry(&self) -> Vec<ScanResult> {
        let mut results = Vec::new();
        let targets = [
            (HKEY_CURRENT_USER, "User Startup"),
            (HKEY_LOCAL_MACHINE, "Machine Startup")
        ];

        let path_str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0";
        let path_u16: Vec<u16> = path_str.encode_utf16().collect();

        for (hive, label) in targets {
            unsafe {
                let mut hkey = HKEY::default();
                if RegOpenKeyExW(hive, PCWSTR(path_u16.as_ptr()), 0, KEY_READ, &mut hkey).is_ok() {
                    let mut index = 0;
                    let mut name_buf = [0u16; 256];
                    let mut name_len = name_buf.len() as u32;

                    while RegEnumValueW(
                        hkey, index, PWSTR(name_buf.as_mut_ptr()), &mut name_len,
                        None, None, None, None
                    ).is_ok() {
                        let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                        results.push(ScanResult {
                            category: "Software Causes".into(),
                            component: label.into(),
                            value: name,
                            raw_value: None,
                            severity: Severity::Medium,
                            description: "Identified in Windows Run Registry".into(),
                            endpoint: r"Win32::RegEnumValueW (Runs)".into(),
                            pid: None,
                        });
                        index += 1;
                        name_len = name_buf.len() as u32;
                    }
                    let _ = RegCloseKey(hkey);
                }
            }
        }
        results
    }

    pub fn scan_background_services(&self) -> Vec<ScanResult> {
        let mut results = Vec::new();
        let mut sys = System::new();
        sys.refresh_processes();

        let targets = [
            ("MsMpEng.exe", "Windows Defender", "Active Antivirus Scanning"),
            ("SearchIndexer.exe", "Windows Search Indexer", "Background File Indexing"),
        ];

        for (exe, name, desc) in targets {
            let is_running = sys.processes().values().any(|p| p.name() == exe);
            if is_running {
                results.push(ScanResult {
                    category: "Software Causes".into(),
                    component: name.into(),
                    value: "Active".into(),
                    raw_value: None,
                    severity: Severity::Low,
                    description: desc.into(),
                    endpoint: "sysinfo::System::processes".into(),
                    pid: None,
                });
            }
        }
        results
    }

    pub fn scan_os_and_drivers(&self) -> Vec<ScanResult> {
        let mut results = Vec::new();

        let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
        if let Ok(sub_key) = hklm.open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") {
            let product_name: String = sub_key.get_value("ProductName").unwrap_or_else(|e| {
                log::warn!("Registry ProductName: {:?}", e); "Windows".into()
            });
            let build_number: String = sub_key.get_value("CurrentBuild").unwrap_or_else(|e| {
                log::warn!("Registry CurrentBuild: {:?}", e); "Unknown".into()
            });
            let display_version: String = sub_key.get_value("DisplayVersion").unwrap_or_default();

            results.push(ScanResult {
                category: "Software Causes".into(),
                component: "OS Version".into(),
                value: format!("{} {} (Build {})", product_name, display_version, build_number),
                raw_value: None,
                severity: Severity::Low,
                description: "High-accuracy version retrieved via Registry.".into(),
                endpoint: r"winreg (CurrentVersion)".into(),
                pid: None,
            });
        }

        unsafe {
            let mut dev_node: u32 = 0;
            if CM_Locate_DevNodeW(&mut dev_node, None, CM_LOCATE_DEVNODE_NORMAL) == CR_SUCCESS {
                let mut status = CM_DEVNODE_STATUS_FLAGS::default();
                let mut problem_code = CM_PROB::default();

                if CM_Get_DevNode_Status(&mut status, &mut problem_code, dev_node, 0) == CR_SUCCESS {
                    if problem_code.0 != 0 {
                        results.push(ScanResult {
                            category: "Software Causes".into(),
                            component: "Driver Health".into(),
                            value: format!("Problem Code: {}", problem_code.0),
                            raw_value: Some(problem_code.0 as f64),
                            severity: Severity::High,
                            description: format!("Device reported problem code {}. Check Device Manager.", problem_code.0),
                            endpoint: "Win32::CM_Get_DevNode_Status".into(),
                            pid: None,
                        });
                    } else {
                        results.push(ScanResult {
                            category: "Software Causes".into(),
                            component: "Driver Health".into(),
                            value: "Healthy".into(),
                            raw_value: Some(0.0),
                            severity: Severity::Low,
                            description: "No PnP device problems detected at root.".into(),
                            endpoint: "Win32::CM_Get_DevNode_Status".into(),
                            pid: None,
                        });
                    }
                }
            }
        }

        results
    }
}

impl Scannable for SoftwareScanner {
    fn scan(&mut self) -> Vec<ScanResult> {
        let mut all = self.scan_startup_registry();
        all.extend(self.scan_background_services());
        all.extend(self.scan_os_and_drivers());
        all
    }
}
