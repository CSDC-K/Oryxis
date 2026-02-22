
use pyo3::prelude::*;
use pyo3::types::*;
use std::sync::Once;

static PYTHON_INIT: Once = Once::new();

pub fn handle_general_execute(code: String) -> PyResult<String> {
    PYTHON_INIT.call_once(|| {
        pyo3::prepare_freethreaded_python();
    });

    Python::with_gil(|py| {
        let sys = py.import("sys")?;

        // root and libraries path adding to sys.path
        let project_root = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let path_list: Vec<String> = sys.getattr("path")?.extract()?;
        if !path_list.contains(&project_root) {
            sys.getattr("path")?.call_method1("insert", (0, &project_root))?;
        }  

        let libs_path = std::env::current_dir()
            .unwrap_or_default()
            .join("libraries") // Alt klasöre in
            .to_string_lossy()
            .to_string();

        // add to sys.path
        let path_list: Vec<String> = sys.getattr("path")?.extract()?;
        if !path_list.contains(&libs_path) {
            sys.getattr("path")?.call_method1("insert", (0, &libs_path))?;
        }

        // stdout capture
        let io_mod = py.import("io")?;
        let string_io = io_mod.getattr("StringIO")?.call0()?;
        let original_stdout = sys.getattr("stdout")?;
        sys.setattr("stdout", &string_io)?;

        // Globals dict
        let globals = PyDict::new(py);
        let builtins = py.import("builtins")?;
        globals.set_item("__builtins__", builtins)?;

        let trimmed = code.trim();
        let lines: Vec<&str> = trimmed.lines().collect();

        let result: PyResult<Py<PyAny>> = if lines.len() <= 1 {
            let code_cstr = std::ffi::CString::new(trimmed).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())
            })?;
            match py.eval(&code_cstr, Some(&globals), None) {
                Ok(val) => Ok(val.into()),
                Err(_) => {
                    py.run(&code_cstr, Some(&globals), None)?;
                    Ok(py.None())
                }
            }
        } else {
            let last_line = lines[lines.len() - 1].trim();
            let body = lines[..lines.len() - 1].join("\n");

            let body_cstr = std::ffi::CString::new(body.as_str()).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())
            })?;
            py.run(&body_cstr, Some(&globals), None)?;

            let last_cstr = std::ffi::CString::new(last_line).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())
            })?;
            match py.eval(&last_cstr, Some(&globals), None) {
                Ok(val) => Ok(val.into()),
                Err(_) => {
                    py.run(&last_cstr, Some(&globals), None)?;
                    Ok(py.None())
                }
            }
        };

        // stdout restore
        let captured_output: String = string_io.call_method0("getvalue")?.extract()?;
        sys.setattr("stdout", &original_stdout)?;

        match result {
            Ok(val) => {
                let val_ref = val.bind(py);
                if val_ref.is_none() {
                    if captured_output.is_empty() {
                        Ok("None".to_string())
                    } else {
                        Ok(captured_output.trim().to_string())
                    }
                } else {
                    let repr: String = val_ref.repr()?.extract()?;
                    if captured_output.is_empty() {
                        Ok(repr)
                    } else {
                        Ok(format!("{}\n[Return]: {}", captured_output.trim(), repr))
                    }
                }
            }
            Err(e) => {
                sys.setattr("stdout", &original_stdout).ok();
                let err_msg = format!("Python Error: {}", e);
                if !captured_output.is_empty() {
                    Ok(format!("{}\n{}", captured_output.trim(), err_msg))
                } else {
                    Ok(err_msg)
                }
            }
        }
    })
}


pub fn handle_fast_execute(event_code: &str) -> String {
    let python_code = if event_code.starts_with("_event_CHECKSKILLS->") {
        let filter_part = event_code.trim_start_matches("_event_CHECKSKILLS->").trim();
        let keywords: Vec<&str> = filter_part.split(',').map(|s| s.trim()).collect();
        let keywords_py: Vec<String> = keywords.iter().map(|k| format!("'{}'", k.to_lowercase())).collect();
        format!(
r#"import memory

def task():
    db = memory.OryxisMemory('./mydb')
    skills = db.list_skills()
    keywords = [{}]
    filtered = [s for s in skills if any(kw in s.get('name','').lower() or kw in s.get('description','').lower() for kw in keywords)]
    return {{'status': 'success', 'count': len(filtered), 'skills': filtered}}

task()"#,
            keywords_py.join(", ")
        )
    } else if event_code.starts_with("_event_CHECKSKILLS_") {
        r#"import memory

def task():
    db = memory.OryxisMemory('./mydb')
    skills = db.list_skills()
    return {'status': 'success', 'count': len(skills), 'skills': skills}

task()"#.to_string()
    } else if event_code.starts_with("_event_GETSKILL->") {
        let skill_name = event_code.trim_start_matches("_event_GETSKILL->").trim();
        format!(
r#"import memory

def task():
    db = memory.OryxisMemory('./mydb')
    skill = db.get_skill('{}')
    if skill:
        return {{'status': 'success', 'skill': skill}}
    return {{'status': 'error', 'message': 'Skill not found: {}'}}

task()"#,
            skill_name, skill_name
        )
    } else if event_code.starts_with("_event_DELETESKILL->") {
        let skill_name = event_code.trim_start_matches("_event_DELETESKILL->").trim();
        format!(
r#"import memory

def task():
    db = memory.OryxisMemory('./mydb')
    db.delete_skill('{}')
    return {{'status': 'success', 'deleted': '{}'}}

task()"#,
            skill_name, skill_name
        )
    } else if event_code.starts_with("cmdlib.run_command") {
        format!(
r#"import cmdlib

def task():
    result = {}
    return {{'status': 'success', 'result': result}}

task()"#,
            event_code
        )
    } else {
        return format!("{{\"status\": \"error\", \"message\": \"Unknown fast_execute event: {}\"}}", event_code);
    };

    match handle_general_execute(python_code) {
        Ok(result) => result,
        Err(e) => format!("{{\"status\": \"error\", \"message\": \"{}\"}}", e),
    }
}