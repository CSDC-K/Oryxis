use pyo3::prelude::*;
use std::process::{Command};

#[pyfunction]
pub fn run_command(cmd: String, args: Vec<String>) -> PyResult<String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Ok(format!("ERROR: {}", String::from_utf8_lossy(&output.stderr)))
    }
}


#[pymodule]
fn cmdlib(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Fonksiyonu modüle bu şekilde ekliyoruz:
    m.add_function(wrap_pyfunction!(run_command, m)?)?;
    Ok(())
}
