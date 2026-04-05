pub mod toon;

#[cfg(feature = "python")]
mod py_binding {
    use crate::toon::{encode_with, Config};
    use pyo3::prelude::*;
    use pyo3::types::PyBytes;

    #[pyfunction]
    #[pyo3(signature = (json_bytes, delimiter=",", key_folding=false, flatten_depth=None))]
    fn dumps_bytes<'py>(
        py: Python<'py>,
        json_bytes: &Bound<'py, PyBytes>,
        delimiter: &str,
        key_folding: bool,
        flatten_depth: Option<usize>,
    ) -> PyResult<String> {
        let delim = delimiter.as_bytes().first().copied().unwrap_or(b',');
        if !matches!(delim, b',' | b'\t' | b'|') {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "delimiter must be ',', '\\t', or '|'",
            ));
        }
        let cfg = Config {
            delimiter: delim,
            key_folding,
            flatten_depth,
        };
        let bytes = json_bytes.as_bytes();
        py.detach(|| encode_with(bytes, &cfg))
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    #[pymodule]
    fn _etoon(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_function(wrap_pyfunction!(dumps_bytes, m)?)?;
        Ok(())
    }
}
