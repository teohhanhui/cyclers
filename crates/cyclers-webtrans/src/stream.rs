use std::error::Error;
use std::fmt;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use std::hash::{Hash, Hasher};
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use std::rc::Rc;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use js_sys::Symbol;
use tokio::sync::RwLock;
#[cfg(not(target_family = "wasm"))]
use web_transport_quinn::{ReadError, RecvStream, SendStream, SessionError, quinn};
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use web_transport_wasm::{RecvStream, SendStream};

#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct StreamId(pub(crate) quinn::StreamId);

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
#[derive(Clone, Debug)]
pub struct StreamId(pub(crate) Rc<Symbol>);

pub(crate) enum Stream {
    OutgoingUni(RwLock<SendStream>),
    OutgoingBi(RwLock<SendStream>, RwLock<RecvStream>),
    IncomingUni(RwLock<RecvStream>),
    IncomingBi(RwLock<SendStream>, RwLock<RecvStream>),
}

/// The error type returned when creating a stream fails.
#[derive(Debug)]
#[non_exhaustive]
pub struct CreateStreamError {
    pub(crate) kind: CreateStreamErrorKind,
    #[cfg(not(target_family = "wasm"))]
    pub(crate) inner: Box<dyn Error + Send + Sync + 'static>,
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    pub(crate) inner: Box<dyn Error + 'static>,
}

/// The various types of errors that can cause creating a stream to fail.
#[derive(Debug)]
#[non_exhaustive]
pub enum CreateStreamErrorKind {
    /// Session not found.
    SessionNotFound,
    /// Transport state is "closed" or "failed".
    InvalidTransportState,
}

/// The error type returned when receiving a stream fails.
#[derive(Debug)]
#[non_exhaustive]
pub struct ReceiveStreamError {
    pub(crate) kind: ReceiveStreamErrorKind,
    #[cfg(not(target_family = "wasm"))]
    pub(crate) inner: Box<dyn Error + Send + Sync + 'static>,
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    pub(crate) inner: Box<dyn Error + 'static>,
}

/// The various types of errors that can cause receiving a stream to fail.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReceiveStreamErrorKind {
    /// Session not found.
    SessionNotFound,
    /// Transport state is "closed" or "failed".
    InvalidTransportState,
}

/// The error type returned when receiving bytes fails.
#[derive(Debug)]
#[non_exhaustive]
pub struct ReceiveBytesError {
    pub(crate) kind: ReceiveBytesErrorKind,
    #[cfg(not(target_family = "wasm"))]
    pub(crate) inner: Box<dyn Error + Send + Sync + 'static>,
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    pub(crate) inner: Box<dyn Error + 'static>,
}

/// The various types of errors that can cause receiving bytes to fail.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReceiveBytesErrorKind {
    /// Stream not found.
    StreamNotFound,
    /// Stream is not a readable stream.
    NonReadableStream,
    /// Error when reading from stream.
    Read,
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
impl Eq for StreamId {}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
impl PartialEq for StreamId {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
impl Hash for StreamId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (&*self.0 as *const _ as usize).hash(state)
    }
}

impl fmt::Display for CreateStreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            CreateStreamErrorKind::SessionNotFound => {
                let err = &self.inner;
                write!(f, "session not found: {err}")
            },
            CreateStreamErrorKind::InvalidTransportState => {
                #[cfg(not(target_family = "wasm"))]
                let err = self.inner.downcast_ref::<SessionError>().unwrap();
                #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                let err = self
                    .inner
                    .downcast_ref::<web_transport_wasm::Error>()
                    .unwrap();
                write!(f, "transport state is \"closed\" or \"failed\": {err}")
            },
        }
    }
}

impl Error for CreateStreamError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.kind {
            CreateStreamErrorKind::SessionNotFound => None,
            CreateStreamErrorKind::InvalidTransportState => {
                #[cfg(not(target_family = "wasm"))]
                let err = self.inner.downcast_ref::<SessionError>().unwrap();
                #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                let err = self
                    .inner
                    .downcast_ref::<web_transport_wasm::Error>()
                    .unwrap();
                Some(err)
            },
        }
    }
}

impl CreateStreamError {
    /// Returns the corresponding [`CreateStreamErrorKind`] for this error.
    #[must_use]
    pub const fn kind(&self) -> &CreateStreamErrorKind {
        &self.kind
    }
}

impl fmt::Display for ReceiveStreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ReceiveStreamErrorKind::SessionNotFound => {
                let err = &self.inner;
                write!(f, "session not found: {err}")
            },
            ReceiveStreamErrorKind::InvalidTransportState => {
                #[cfg(not(target_family = "wasm"))]
                let err = self.inner.downcast_ref::<SessionError>().unwrap();
                #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                let err = self
                    .inner
                    .downcast_ref::<web_transport_wasm::Error>()
                    .unwrap();
                write!(f, "transport state is \"closed\" or \"failed\": {err}")
            },
        }
    }
}

impl Error for ReceiveStreamError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.kind {
            ReceiveStreamErrorKind::SessionNotFound => None,
            ReceiveStreamErrorKind::InvalidTransportState => {
                #[cfg(not(target_family = "wasm"))]
                let err = self.inner.downcast_ref::<SessionError>().unwrap();
                #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                let err = self
                    .inner
                    .downcast_ref::<web_transport_wasm::Error>()
                    .unwrap();
                Some(err)
            },
        }
    }
}

impl ReceiveStreamError {
    /// Returns the corresponding [`ReceiveStreamErrorKind`] for this error.
    #[must_use]
    pub const fn kind(&self) -> &ReceiveStreamErrorKind {
        &self.kind
    }
}

impl fmt::Display for ReceiveBytesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ReceiveBytesErrorKind::StreamNotFound => {
                let err = &self.inner;
                write!(f, "stream not found: {err}")
            },
            ReceiveBytesErrorKind::NonReadableStream => {
                let err = &self.inner;
                write!(f, "stream is not a readable stream: {err}")
            },
            ReceiveBytesErrorKind::Read => {
                #[cfg(not(target_family = "wasm"))]
                let err = self.inner.downcast_ref::<ReadError>().unwrap();
                #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                let err = self
                    .inner
                    .downcast_ref::<web_transport_wasm::Error>()
                    .unwrap();
                write!(f, "error when reading from stream: {err}")
            },
        }
    }
}

impl Error for ReceiveBytesError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.kind {
            ReceiveBytesErrorKind::StreamNotFound => None,
            ReceiveBytesErrorKind::NonReadableStream => None,
            ReceiveBytesErrorKind::Read => {
                #[cfg(not(target_family = "wasm"))]
                let err = self.inner.downcast_ref::<ReadError>().unwrap();
                #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                let err = self
                    .inner
                    .downcast_ref::<web_transport_wasm::Error>()
                    .unwrap();
                Some(err)
            },
        }
    }
}

impl ReceiveBytesError {
    /// Returns the corresponding [`ReceiveBytesErrorKind`] for this error.
    #[must_use]
    pub const fn kind(&self) -> &ReceiveBytesErrorKind {
        &self.kind
    }
}
