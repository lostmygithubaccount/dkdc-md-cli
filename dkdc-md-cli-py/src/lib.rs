use pyo3::prelude::*;

#[pyfunction]
fn run(argv: Vec<String>) -> PyResult<()> {
    dkdc_md_cli::run(argv.iter().map(|s| s.as_str()))
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
}

#[pymodule]
fn core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run, m)?)?;
    Ok(())
}
