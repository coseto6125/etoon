pub mod toon;

#[cfg(feature = "python")]
mod py_binding {
    //! The options seam. `Config` holds the defaults; this adapter maps the
    //! Python keyword surface onto it and nothing more.
    use crate::toon::{encode_with, Config};
    use pyo3::exceptions::{PyTypeError, PyValueError};
    use pyo3::prelude::*;
    use pyo3::types::{PyBytes, PyDict};

    /// Keyword names accepted by [`dumps_bytes`]; anything else is rejected
    /// the way flat parameters would reject it.
    const OPTION_NAMES: [&str; 7] = [
        "delimiter",
        "key_folding",
        "flatten_depth",
        "empty_array_bare",
        "escape_controls",
        "max_depth",
        "max_input_bytes",
    ];

    /// Read one optional keyword into `dest`; a missing name or an explicit
    /// `None` leaves the Config default in place.
    macro_rules! opt_kw {
        ($kwargs:expr, $name:literal, $dest:expr, $ty:ty) => {
            if let Some(v) = $kwargs.get_item($name)? {
                if !v.is_none() {
                    $dest = v.extract::<$ty>()?;
                }
            }
        };
    }

    /// Map caller kwargs onto a `Config`. Defaults come solely from
    /// `Config::default`; this function is the only place that knows the
    /// Python-side names.
    fn config_from_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Config> {
        let mut cfg = Config::default();
        let Some(kwargs) = kwargs else {
            return Ok(cfg);
        };
        for k in kwargs.keys().iter() {
            let name: String = k.extract()?;
            if !OPTION_NAMES.contains(&name.as_str()) {
                return Err(PyTypeError::new_err(format!(
                    "unexpected keyword argument '{name}'"
                )));
            }
        }
        if let Some(v) = kwargs.get_item("delimiter")? {
            if !v.is_none() {
                let d: String = v.extract()?;
                // Exact match: an empty or multi-character delimiter must
                // raise instead of silently falling back to its first byte.
                cfg.delimiter = match d.as_bytes() {
                    b"," => b',',
                    b"\t" => b'\t',
                    b"|" => b'|',
                    _ => {
                        return Err(PyValueError::new_err(
                            "delimiter must be ',', '\\t', or '|'",
                        ))
                    }
                };
            }
        }
        opt_kw!(kwargs, "key_folding", cfg.key_folding, bool);
        opt_kw!(kwargs, "flatten_depth", cfg.flatten_depth, Option<usize>);
        opt_kw!(kwargs, "empty_array_bare", cfg.empty_array_bare, bool);
        opt_kw!(kwargs, "escape_controls", cfg.escape_controls, bool);
        opt_kw!(kwargs, "max_depth", cfg.max_depth, usize);
        opt_kw!(kwargs, "max_input_bytes", cfg.max_input_bytes, usize);
        Ok(cfg)
    }

    #[pyfunction(signature = (json_bytes, **kwargs))]
    fn dumps_bytes<'py>(
        py: Python<'py>,
        json_bytes: &Bound<'py, PyBytes>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<String> {
        let cfg = config_from_kwargs(kwargs)?;
        let bytes = json_bytes.as_bytes();
        py.detach(|| encode_with(bytes, &cfg))
            .map_err(PyValueError::new_err)
    }

    #[pymodule]
    fn _etoon(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_function(wrap_pyfunction!(dumps_bytes, m)?)?;
        Ok(())
    }
}
