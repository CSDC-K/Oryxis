use std::sync::Arc;
use once_cell::sync::Lazy;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde::{Serialize, Deserialize};
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, RocksDb};
use tokio::runtime::Runtime;

// --- Veri Yapısı ---
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Skill {
    name: String,
    description: String,
    code: String,
}

// --- Statik Nizam (Kilitlenmeyi Önleyen Kısım) ---
// DB ve Runtime'ı statik yaparak dosyanın sadece bir kez açılmasını sağlıyoruz.
static DB_INSTANCE: Lazy<Surreal<Db>> = Lazy::new(Surreal::init);
static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    Runtime::new().expect("[ORYXIS-ERROR] Tokio Runtime başlatılamadı!")
});

#[pyclass]
struct OryxisMemory;

#[pymethods]
impl OryxisMemory {
    #[new]
    pub fn new(path: String) -> PyResult<Self> {
        // block_on içinde DB'yi sadece ilk seferde başlatıyoruz
        RUNTIME.block_on(async {
            // Eğer veritabanı henüz bir motora bağlanmadıysa bağla
            // Bu kontrol RocksDB dosya kilidinin çift açılmasını engeller
            let _ = DB_INSTANCE.connect::<RocksDb>(&path).await;
            
            let _ = DB_INSTANCE
                .use_ns("oryxis")
                .use_db("skills")
                .await;
        });
        Ok(OryxisMemory)
    }

    pub fn add_skill(&self, name: String, desc: String, code: String) -> PyResult<String> {
        RUNTIME.block_on(async {
            let _: Option<Skill> = DB_INSTANCE
                .create(("skills", &name))
                .content(Skill {
                    name: name.clone(),
                    description: desc,
                    code,
                })
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            Ok(format!("[ORYXIS-MEMORY] New Skill Saved: {}", name))
        })
    }

    pub fn list_skills(&self) -> PyResult<Vec<PyObject>> {
        RUNTIME.block_on(async {
            let skills: Vec<Skill> = DB_INSTANCE
                .select("skills")
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            Python::with_gil(|py| {
                let mut results = Vec::new();
                for skill in skills {
                    let dict = PyDict::new(py);
                    dict.set_item("name", skill.name)?;
                    dict.set_item("description", skill.description)?;
                    dict.set_item("code", skill.code)?;
                    results.push(dict.into());
                }
                Ok(results)
            })
        })
    }

    pub fn edit_skill(&self, name: String, new_description: Option<String>, new_code: Option<String>) -> PyResult<String> {
        RUNTIME.block_on(async {
            let mut update_data = std::collections::HashMap::new();
            if let Some(desc) = new_description { update_data.insert("description", desc); }
            if let Some(code) = new_code { update_data.insert("code", code); }

            if update_data.is_empty() {
                return Ok(format!("[ORYXIS-MEMORY] No changes for Skill: {}", name));
            }

            let _: Option<Skill> = DB_INSTANCE
                .update(("skills", &name))
                .merge(update_data)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            Ok(format!("[ORYXIS-MEMORY] Skill Updated: {}", name))
        })
    }

    pub fn delete_skill(&self, name: String) -> PyResult<String> {
        RUNTIME.block_on(async {
            let _: Option<Skill> = DB_INSTANCE
                .delete(("skills", &name))
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            Ok(format!("[ORYXIS-MEMORY] Skill Deleted: {}", name))
        })
    }
}

#[pymodule]
fn memory(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<OryxisMemory>()?;
    Ok(())
}