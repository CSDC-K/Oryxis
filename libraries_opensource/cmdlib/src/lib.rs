use pyo3::prelude::*;
use std::process::Command;
use std::thread;


fn execute_command(cmd: String, args: Vec<String>) -> Result<String, String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!("ERROR: {}", String::from_utf8_lossy(&output.stderr)))
    }
}

#[pyfunction]
// py: Python parametresini eklemelisin
pub fn run_command(py: Python<'_>, cmd: String, args: Vec<String>) -> PyResult<String> {

    let result = py.allow_threads(move || {
        thread::spawn(move || {
            execute_command(cmd, args)
        }).join().map_err(|_| "Thread panicked")?
    });

    // Result'ı PyResult'a çeviriyoruz
    result.map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))
}

#[pymodule]
fn cmdlib(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // run_command fonksiyonunu modüle ekliyoruz
    m.add_function(wrap_pyfunction!(run_command, m)?)?;
    Ok(())
}