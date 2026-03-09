use serde::{Serialize, Deserialize};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;


#[derive(Serialize, Deserialize)]
struct SkillIndex {
    name: String,
    description: String,
    tags: Vec<String>,
    file: String,
}


#[no_mangle]
pub extern "C" fn get_skill_index(tags_json: *const c_char) -> *mut c_char {
    let result = _get_skill_index(tags_json);
    CString::new(result).unwrap_or_default().into_raw()
}

fn _get_skill_index(tags_json: *const c_char) -> String {
    let tags_str = unsafe {
        if tags_json.is_null() { return "[]".to_string(); }
        match CStr::from_ptr(tags_json).to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return "[]".to_string(),
        }
    };

    let tags: Vec<String> = match serde_json::from_str(&tags_str) {
        Ok(t) => t,
        Err(e) => return format!(r#"{{"error":"invalid tags json: {}"}}"#, e),
    };

    let json_str = match std::fs::read_to_string("memory/skills_index.json") {
        Ok(s) => s,
        Err(_) => {
            // Fallback: try relative to exe
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()));
            match exe_dir {
                Some(d) => match std::fs::read_to_string(d.join("memory/skills_index.json")) {
                    Ok(s) => s,
                    Err(e) => return format!(r#"{{"error":"{}"}}"#, e),
                },
                None => return r#"{"error":"skills_index.json not found"}"#.to_string(),
            }
        }
    };

    let skills: Vec<SkillIndex> = match serde_json::from_str(&json_str) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error":"parse error: {}"}}"#, e),
    };

    let tags_lower: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();
    let filtered: Vec<&SkillIndex> = skills.iter().filter(|s| {
        tags_lower.iter().any(|tag| {
            s.tags.iter().any(|t| t.to_lowercase().contains(tag.as_str()))
                || s.name.to_lowercase().contains(tag.as_str())
                || s.description.to_lowercase().contains(tag.as_str())
        })
    }).collect();

    serde_json::to_string_pretty(&filtered).unwrap_or("[]".to_string())
}

#[no_mangle]
pub extern "C" fn get_yaml_content(path: *const c_char) -> *mut c_char {
    let result = unsafe {
        if path.is_null() { return CString::new("").unwrap().into_raw(); }
        match CStr::from_ptr(path).to_str() {
            Ok(p) => std::fs::read_to_string(p).unwrap_or_else(|e| format!(r#"{{"error":"{}"}}"#, e)),
            Err(_) => String::new(),
        }
    };
    CString::new(result).unwrap_or_default().into_raw()
}

#[no_mangle]
pub extern "C" fn get_all_index() -> *mut c_char {
    let result = std::fs::read_to_string("memory/skills_index.json")
        .unwrap_or_else(|e| format!(r#"{{"error":"{}"}}"#, e));
    CString::new(result).unwrap_or_default().into_raw()
}

#[no_mangle]
pub extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { let _ = CString::from_raw(ptr); }
    }
}