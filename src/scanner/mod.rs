pub mod software;
pub mod hardware;
pub mod usage;
pub mod worker;

use crate::models::ScanResult;

pub trait Scannable {
    fn scan(&mut self) -> Vec<ScanResult>;
}
