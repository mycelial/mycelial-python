use mycelial_crdt::{list, vclock};
use pyo3::exceptions;
use pyo3::prelude::*;
use pyo3::types;

#[derive(Debug)]
#[pyclass]
pub struct List(list::List);

#[derive(Debug)]
#[repr(transparent)]
pub struct WrappedValue<Key>(list::Value<Key>);

impl<'a, Key> From<&'a list::Value<Key>> for &'a WrappedValue<Key> {
    fn from(v: &'a list::Value<Key>) -> Self {
        unsafe { &*(v as *const list::Value<Key> as *const WrappedValue<Key>) }
    }
}

#[allow(clippy::from_over_into)]
impl<Key> Into<list::Value<Key>> for WrappedValue<Key> {
    fn into(self) -> list::Value<Key> {
        self.0
    }
}

impl<Key> ToPyObject for WrappedValue<Key> {
    fn to_object(&self, py: Python<'_>) -> PyObject {
        match self.0 {
            list::Value::Bool(b) => b.to_object(py),
            list::Value::Float(f) => f.to_object(py),
            list::Value::Str(ref s) => s.as_str().to_object(py),
            list::Value::Vec(ref v) => v
                .iter()
                .map(|v| {
                    let v: &WrappedValue<Key> = v.into();
                    v.to_object(py)
                })
                .collect::<Vec<_>>()
                .to_object(py),
            _ => unreachable!(),
        }
    }
}

impl<'source, Key> FromPyObject<'source> for WrappedValue<Key> {
    fn extract(obj: &'source types::PyAny) -> PyResult<Self> {
        if let Ok(b) = obj.downcast::<types::PyBool>() {
            return Ok(WrappedValue(list::Value::Bool(b.is_true())));
        }
        if let Ok(f) = obj.downcast::<types::PyFloat>() {
            return Ok(WrappedValue(list::Value::Float(f.value() as f64)));
        }
        if let Ok(s) = obj.downcast::<types::PyString>() {
            return Ok(WrappedValue(list::Value::Str(s.to_str()?.into())));
        }
        if let Ok(l) = obj.downcast::<types::PyList>() {
            let vec = l
                .iter()
                .map(|v| {
                    let v: PyResult<WrappedValue<Key>> = v.extract();
                    v.map(|v| v.into())
                })
                .collect::<PyResult<Vec<list::Value<Key>>>>()?;
            return Ok(WrappedValue(list::Value::Vec(vec)));
        }
        Err(exceptions::PyValueError::new_err("unsupported value"))
    }
}

fn to_error(e: impl std::error::Error) -> PyErr {
    exceptions::PyValueError::new_err(format!("{:?}", e))
}

#[pymethods]
impl List {
    #[new]
    fn new(id: u64) -> Self {
        Self(list::List::new(id))
    }

    fn append(&mut self, py: Python<'_>, obj: PyObject) -> PyResult<()> {
        let value: WrappedValue<_> = obj.extract(py)?;
        self.0.append(value.into()).map_err(to_error)
    }

    fn prepend(&mut self, py: Python<'_>, obj: PyObject) -> PyResult<()> {
        let value: WrappedValue<_> = obj.extract(py)?;
        self.0.prepend(value.into()).map_err(to_error)
    }

    fn delete(&mut self, index: usize) -> PyResult<()> {
        self.0.delete(index).map_err(to_error)
    }

    fn insert(&mut self, py: Python<'_>, index: usize, obj: PyObject) -> PyResult<()> {
        let value: WrappedValue<_> = obj.extract(py)?;
        self.0.insert(index, value.into()).map_err(to_error)
    }

    fn vclock(&self, py: Python<'_>) -> PyResult<PyObject> {
        let encoded = serde_json::to_string(self.0.vclock()).map_err(to_error)?;
        Ok(encoded.to_object(py))
    }

    fn diff(&self, py: Python<'_>, vclock: PyObject) -> PyResult<PyObject> {
        if let Ok(encoded) = vclock.cast_as::<types::PyString>(py) {
            let vc: vclock::VClock =
                serde_json::from_str(encoded.to_str()?).map_err(to_error)?;
            let diff = serde_json::to_string(&self.0.diff(&vc)).map_err(to_error)?;
            return Ok(diff.to_object(py));
        }
        Err(exceptions::PyValueError::new_err("bad vclock"))
    }

    fn apply(&mut self, py: Python<'_>, diff: PyObject) -> PyResult<()> {
        if let Ok(encoded) = diff.cast_as::<types::PyString>(py) {
            let diff: Vec<_> = serde_json::from_str(encoded.to_str()?).map_err(to_error)?;
            return self.0.apply(&diff).map_err(to_error)
        }
        Err(exceptions::PyValueError::new_err("bad diff"))
    }

    fn to_vec(&self, py: Python<'_>) -> PyResult<PyObject> {
        let l = types::PyList::empty(py);
        for value in self.0.iter() {
            let val: &WrappedValue<_> = value.into();
            l.append(val.to_object(py))?;
        }
        Ok(l.into())
    }

    fn set_on_update(&mut self, _py: Python<'_>, cb: PyObject) {
        self.0.set_on_update(Box::new(move |ops| {
            // FIXME: unwrap & ok()
            let encoded = serde_json::to_string(&[ops]).unwrap();
            Python::with_gil(|py| {
                let tuple = types::PyTuple::new(py, vec![encoded]);
                cb.call1(py, tuple).ok();
            })
        }));
    }

    fn unset_on_update(&mut self, _py: Python<'_>) {
        self.0.unset_on_update()
    }

    fn set_on_apply(&mut self, _py: Python<'_>, cb: PyObject) {
        self.0.set_on_apply(Box::new(move || {
            Python::with_gil(|py| { cb.call0(py).ok(); });
        }));
    }

    fn unset_on_apply(&mut self, _py: Python<'_>) {
        self.0.unset_on_apply()
    }

    fn vclock_diff(&self, py: Python<'_>, vclock: PyObject) -> PyResult<PyObject> {
        if let Ok(encoded) = vclock.cast_as::<types::PyString>(py) {
            let vc: vclock::VClock = serde_json::from_str(encoded.to_str()?).map_err(to_error)?;
            let lrdiff: vclock::VClockDiff = (self.0.vclock(), &vc).into();
            let rldiff: vclock::VClockDiff = (&vc, self.0.vclock()).into();
            return Ok(types::PyTuple::new(py, vec![lrdiff.into_iter().count() != 0, rldiff.into_iter().count() != 0]).into())
        }
        Err(exceptions::PyValueError::new_err("bad vclock"))
    }
}

#[pymodule]
fn mycelial(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<List>()?;
    Ok(())
}
