use pyo3::prelude::*;
use rust_yaml::*;
use serde::{Serialize, Deserialize};


#[derive(Serialize, Deserialize)]
struct SkillIndex {
    name: String,
    description: String,
    tags: Vec<String>,
    file: String,
}


#[pyfunction]
pub fn get_skill_index(tags : Vec<String>) -> PyResult<String> {
    let yaml_str = std::fs::read_to_string("memory/skills_index.json").map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
    let skills: Vec<SkillIndex> = serde_json::from_str(&yaml_str).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

    let filtered_skills: Vec<&SkillIndex> = skills.iter().filter(|skill| {
        tags.iter().any(|tag| skill.tags.contains(tag))
    }).collect();

    serde_json::to_string_pretty(&filtered_skills).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
}

#[pyfunction]
pub fn get_yaml_content(file_path: String) -> PyResult<String> {
    let yaml_str = std::fs::read_to_string(file_path).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
    Ok(yaml_str)
}



#[pymodule]
fn skill_lib(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(get_skill_index, m)?)?;
    m.add_function(wrap_pyfunction!(get_yaml_content, m)?)?;
    Ok(())
}