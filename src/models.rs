use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScanResult {
    pub category: String,
    pub component: String,
    pub value: String,
    /// Pre-parsed numeric reading. Set for disk%, RAM%, cap% etc.
    /// Avoids repeated string parsing in the render loop.
    #[serde(default)]
    pub raw_value: Option<f64>,
    pub severity: Severity,
    pub description: String,
    pub endpoint: String,
    pub pid: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProcessData {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f32,
    pub mem_mb: u64,
}

pub struct SysTickData {
    pub processes: Vec<ProcessData>,
    pub cpu_pct: f32,
}
