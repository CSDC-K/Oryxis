// Debug Info Module
use chrono::Local;

pub fn print_debug_info(msg: &str) {
    println!("[{}] [DEBUG] {}", Local::now().format("%H:%M"), msg);
}