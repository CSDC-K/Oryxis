use pyo3::prelude::*;
use pyo3::types::*;
use std::sync::Once;

static PYTHON_INIT: Once = Once::new();

pub fn execute_script(code: String) -> PyResult<String> {
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