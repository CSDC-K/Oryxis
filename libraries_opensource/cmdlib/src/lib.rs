use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::process::Command;

#[no_mangle]
pub extern "C" fn run_command(cmd: *const c_char, args_json: *const c_char) -> *mut c_char {
    let result = _run_command(cmd, args_json);
    CString::new(result).unwrap_or_default().into_raw()
}

fn _run_command(cmd: *const c_char, args_json: *const c_char) -> String {
    let cmd_str = unsafe {
        if cmd.is_null() { return r#"{"error":"null cmd"}"#.to_string(); }
        match CStr::from_ptr(cmd).to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return r#"{"error":"invalid cmd utf8"}"#.to_string(),
        }
    };

    let args_str = unsafe {
        if args_json.is_null() { "[]".to_string() }
        else { CStr::from_ptr(args_json).to_str().unwrap_or("[]").to_string() }
    };

    let args: Vec<String> = serde_json::from_str(&args_str).unwrap_or_default();

    match Command::new(&cmd_str).args(&args).output() {
        Ok(output) => {
            if output.status.success() {
                String::from_utf8_lossy(&output.stdout).to_string()
            } else {
                format!("ERROR: {}", String::from_utf8_lossy(&output.stderr))
            }
        }
        Err(e) => format!("ERROR: {}", e),
    }
}

#[no_mangle]
pub extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { let _ = CString::from_raw(ptr); }
    }
}