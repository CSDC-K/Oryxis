use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::fs;
use std::io::{Read, Write};
use sha2::{Sha256, Digest};

fn to_str(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() { return None; }
    unsafe { CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string()) }
}

fn ret(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

fn ok_json(msg: &str) -> String { format!(r#"{{"status":"success","message":"{}"}}"#, msg) }
fn err_json(msg: &str) -> String { format!(r#"{{"status":"error","message":"{}"}}"#, msg.replace('"', "'")) }

#[no_mangle]
pub extern "C" fn read_file(path: *const c_char) -> *mut c_char {
    let Some(p) = to_str(path) else { return ret(err_json("null path")); };
    ret(fs::read_to_string(&p).unwrap_or_else(|e| err_json(&e.to_string())))
}

#[no_mangle]
pub extern "C" fn write_file(path: *const c_char, content: *const c_char) -> *mut c_char {
    let (Some(p), Some(c)) = (to_str(path), to_str(content)) else { return ret(err_json("null arg")); };
    ret(match fs::write(&p, &c) {
        Ok(_) => ok_json("written"),
        Err(e) => err_json(&e.to_string()),
    })
}

#[no_mangle]
pub extern "C" fn append_to_file(path: *const c_char, content: *const c_char) -> *mut c_char {
    let (Some(p), Some(c)) = (to_str(path), to_str(content)) else { return ret(err_json("null arg")); };
    ret(match fs::OpenOptions::new().append(true).create(true).open(&p) {
        Ok(mut f) => match writeln!(f, "{}", c) {
            Ok(_) => ok_json("appended"),
            Err(e) => err_json(&e.to_string()),
        },
        Err(e) => err_json(&e.to_string()),
    })
}

#[no_mangle]
pub extern "C" fn delete_file(path: *const c_char) -> *mut c_char {
    let Some(p) = to_str(path) else { return ret(err_json("null path")); };
    ret(match fs::remove_file(&p) {
        Ok(_) => ok_json("deleted"),
        Err(e) => err_json(&e.to_string()),
    })
}

#[no_mangle]
pub extern "C" fn move_file(src: *const c_char, dst: *const c_char) -> *mut c_char {
    let (Some(s), Some(d)) = (to_str(src), to_str(dst)) else { return ret(err_json("null arg")); };
    ret(match fs::rename(&s, &d) {
        Ok(_) => ok_json("moved"),
        Err(e) => err_json(&e.to_string()),
    })
}

#[no_mangle]
pub extern "C" fn copy_file(src: *const c_char, dst: *const c_char) -> *mut c_char {
    let (Some(s), Some(d)) = (to_str(src), to_str(dst)) else { return ret(err_json("null arg")); };
    ret(match fs::copy(&s, &d) {
        Ok(bytes) => format!(r#"{{"status":"success","bytes":{}}}"#, bytes),
        Err(e) => err_json(&e.to_string()),
    })
}

#[no_mangle]
pub extern "C" fn list_directory(path: *const c_char) -> *mut c_char {
    let Some(p) = to_str(path) else { return ret(err_json("null path")); };
    ret(match fs::read_dir(&p) {
        Ok(entries) => {
            let items: Vec<String> = entries
                .filter_map(|e| e.ok())
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                    format!(r#"{{"name":"{}","is_dir":{},"size":{}}}"#, name, is_dir, size)
                })
                .collect();
            format!("[{}]", items.join(","))
        }
        Err(e) => err_json(&e.to_string()),
    })
}

#[no_mangle]
pub extern "C" fn create_directory(path: *const c_char) -> *mut c_char {
    let Some(p) = to_str(path) else { return ret(err_json("null path")); };
    ret(match fs::create_dir_all(&p) {
        Ok(_) => ok_json("created"),
        Err(e) => err_json(&e.to_string()),
    })
}

#[no_mangle]
pub extern "C" fn delete_directory(path: *const c_char) -> *mut c_char {
    let Some(p) = to_str(path) else { return ret(err_json("null path")); };
    ret(match fs::remove_dir_all(&p) {
        Ok(_) => ok_json("deleted"),
        Err(e) => err_json(&e.to_string()),
    })
}

#[no_mangle]
pub extern "C" fn create_file(path: *const c_char) -> *mut c_char {
    let Some(p) = to_str(path) else { return ret(err_json("null path")); };
    ret(match fs::File::create(&p) {
        Ok(_) => ok_json("created"),
        Err(e) => err_json(&e.to_string()),
    })
}

#[no_mangle]
pub extern "C" fn path_exists(path: *const c_char) -> *mut c_char {
    let Some(p) = to_str(path) else { return ret("false".to_string()); };
    ret(fs::metadata(&p).is_ok().to_string())
}

#[no_mangle]
pub extern "C" fn is_file(path: *const c_char) -> *mut c_char {
    let Some(p) = to_str(path) else { return ret("false".to_string()); };
    ret(fs::metadata(&p).map(|m| m.is_file()).unwrap_or(false).to_string())
}

#[no_mangle]
pub extern "C" fn is_directory(path: *const c_char) -> *mut c_char {
    let Some(p) = to_str(path) else { return ret("false".to_string()); };
    ret(fs::metadata(&p).map(|m| m.is_dir()).unwrap_or(false).to_string())
}

#[no_mangle]
pub extern "C" fn get_metadata(path: *const c_char) -> *mut c_char {
    let Some(p) = to_str(path) else { return ret(err_json("null path")); };
    ret(match fs::metadata(&p) {
        Ok(m) => {
            let modified = m.modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            format!(r#"{{"size":{},"modified":{},"is_file":{},"is_dir":{}}}"#, m.len(), modified, m.is_file(), m.is_dir())
        }
        Err(e) => err_json(&e.to_string()),
    })
}

#[no_mangle]
pub extern "C" fn get_file_hash(path: *const c_char) -> *mut c_char {
    let Some(p) = to_str(path) else { return ret(err_json("null path")); };
    ret(match fs::File::open(&p) {
        Ok(mut f) => {
            let mut hasher = Sha256::new();
            let mut buf = [0u8; 4096];
            loop {
                match f.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => hasher.update(&buf[..n]),
                    Err(e) => return ret(err_json(&e.to_string())),
                }
            }
            format!("{:x}", hasher.finalize())
        }
        Err(e) => err_json(&e.to_string()),
    })
}

#[no_mangle]
pub extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { let _ = CString::from_raw(ptr); }
    }
}