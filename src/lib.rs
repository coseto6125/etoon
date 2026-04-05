pub mod toon;

#[cfg(feature = "python")]
mod py_binding {
    use pyo3::prelude::*;
    use pyo3::types::PyBytes;

    #[pyfunction]
    fn dumps_bytes<'py>(py: Python<'py>, json_bytes: &Bound<'py, PyBytes>) -> PyResult<String> {
        let bytes = json_bytes.as_bytes();
        py.allow_threads(|| crate::toon::encode(bytes))
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    #[pymodule]
    fn _etoon(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_function(wrap_pyfunction!(dumps_bytes, m)?)?;
        Ok(())
    }
}
