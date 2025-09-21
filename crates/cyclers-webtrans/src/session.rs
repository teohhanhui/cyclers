use std::error::Error;
use std::fmt;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use std::hash::{Hash, Hasher};
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use std::rc::Rc;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use js_sys::Symbol;
#[cfg(not(target_family = "wasm"))]
use web_transport_quinn::ClientError;

#[cfg(not(target_family = "wasm"))]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SessionId(pub(crate) usize);

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
#[derive(Clone, Debug)]
pub struct SessionId(pub(crate) Rc<Symbol>);

/// The error type returned when establishing a session fails.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug)]
#[non_exhaustive]
pub struct EstablishSessionError(pub(crate) ClientError);

/// The error type returned when establishing a session fails.
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
#[derive(Debug)]
#[non_exhaustive]
pub struct EstablishSessionError(pub(crate) web_transport_wasm::Error);

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
impl Eq for SessionId {}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
impl PartialEq for SessionId {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
impl Hash for SessionId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (&*self.0 as *const _ as usize).hash(state)
    }
}

impl fmt::Display for EstablishSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let err = &self.0;
        write!(f, "failed to establish session: {err}")
    }
}

impl Error for EstablishSessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}
