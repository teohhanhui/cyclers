use std::any::type_name_of_val;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use std::cell::RefCell;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use std::convert::Infallible;
use std::fmt;
use std::num::TryFromIntError;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use std::rc::Rc;
#[cfg(not(target_family = "wasm"))]
use std::sync::Arc;

use bytes::Bytes;
use cyclers::BoxError;
use url::Url;
#[cfg(not(target_family = "wasm"))]
use web_transport_quinn::Client;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use web_transport_wasm::Client;

use crate::session::SessionId;
use crate::stream::StreamId;

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum WebTransportCommand {
    ConfigureClient(ConfigureClientCommand),
    /// Create a new WebTransport session given a URI [[RFC3986]] of the
    /// requester.
    ///
    /// An origin [[RFC6454]] MUST be given if the WebTransport session is
    /// coming from a browser client; otherwise, it is OPTIONAL.
    ///
    /// See [The WebTransport Protocol Framework, Section 4.1]
    ///
    /// [RFC3986]: https://datatracker.ietf.org/doc/html/rfc3986
    /// [RFC6454]: https://datatracker.ietf.org/doc/html/rfc6454
    /// [The WebTransport Protocol Framework, Section 4.1]: https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-overview#section-4.1
    EstablishSession(EstablishSessionCommand),
    /// Terminate the session while communicating to the peer an unsigned 32-bit
    /// error code and an error reason string of at most 1024 bytes.
    ///
    /// As soon as the session is terminated, no further application data will
    /// be exchanged on it.
    ///
    /// The error code and string are optional; the default values are 0 and "".
    ///
    /// The delivery of the error code and string MAY be best-effort.
    ///
    /// See [The WebTransport Protocol Framework, Section 4.1]
    ///
    /// [The WebTransport Protocol Framework, Section 4.1]: https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-overview#section-4.1
    TerminateSession(TerminateSessionCommand),
    /// Creates an outgoing unidirectional stream.
    ///
    /// This operation may block until the flow control of the underlying
    /// protocol allows for it to be completed.
    ///
    /// See [The WebTransport Protocol Framework, Section 4.3]
    ///
    /// [The WebTransport Protocol Framework, Section 4.3]: https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-overview#section-4.3
    CreateUniStream(CreateUniStreamCommand),
    /// Creates an outgoing bidirectional stream.
    ///
    /// This operation may block until the flow control of the underlying
    /// protocol allows for it to be completed.
    ///
    /// See [The WebTransport Protocol Framework, Section 4.3]
    ///
    /// [The WebTransport Protocol Framework, Section 4.3]: https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-overview#section-4.3
    CreateBiStream(CreateBiStreamCommand),
    /// Add bytes into the stream send buffer.
    ///
    /// The sender can also indicate a FIN, signalling the fact that no new data
    /// will be sent on the stream.
    ///
    /// Not applicable for incoming unidirectional streams.
    ///
    /// See [The WebTransport Protocol Framework, Section 4.3]
    ///
    /// [The WebTransport Protocol Framework, Section 4.3]: https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-overview#section-4.3
    SendBytes(SendBytesCommand),
}

#[derive(Clone)]
pub struct ConfigureClientCommand(pub(crate) ConfigureClientFn);

#[cfg(not(target_family = "wasm"))]
type ConfigureClientFn = Arc<
    std::sync::Mutex<
        Option<Box<dyn FnOnce() -> Result<Client, ConfigureClientError> + Send + Sync>>,
    >,
>;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
type ConfigureClientFn =
    Rc<RefCell<Option<Box<dyn FnOnce() -> Result<Client, ConfigureClientError>>>>>;

#[cfg(not(target_family = "wasm"))]
pub(crate) type ConfigureClientError = web_transport_quinn::ClientError;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub(crate) type ConfigureClientError = Infallible;

/// Create a new WebTransport session given a URI [[RFC3986]] of the
/// requester.
///
/// An origin [[RFC6454]] MUST be given if the WebTransport session is
/// coming from a browser client; otherwise, it is OPTIONAL.
///
/// See [The WebTransport Protocol Framework, Section 4.1]
///
/// [RFC3986]: https://datatracker.ietf.org/doc/html/rfc3986
/// [RFC6454]: https://datatracker.ietf.org/doc/html/rfc6454
/// [The WebTransport Protocol Framework, Section 4.1]: https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-overview#section-4.1
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct EstablishSessionCommand {
    pub url: Url,
}

/// Terminate the session while communicating to the peer an unsigned 32-bit
/// error code and an error reason string of at most 1024 bytes.
///
/// As soon as the session is terminated, no further application data will
/// be exchanged on it.
///
/// The error code and string are optional; the default values are 0 and "".
///
/// The delivery of the error code and string MAY be best-effort.
///
/// See [The WebTransport Protocol Framework, Section 4.1]
///
/// [The WebTransport Protocol Framework, Section 4.1]: https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-overview#section-4.1
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct TerminateSessionCommand {
    pub session_id: SessionId,
    pub code: ErrorCode,
    pub reason: ErrorReason,
}

/// An unsigned 32-bit error code.
///
/// See [The WebTransport Protocol Framework, Section 4.3]
///
/// [The WebTransport Protocol Framework, Section 4.3]: https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-overview#section-4.3
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct ErrorCode(pub(crate) u32);

/// An error reason string of at most 1024 bytes.
///
/// See [The WebTransport Protocol Framework, Section 4.3]
///
/// [The WebTransport Protocol Framework, Section 4.3]: https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-overview#section-4.3
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct ErrorReason(pub(crate) String);

/// Creates an outgoing unidirectional stream.
///
/// This operation may block until the flow control of the underlying
/// protocol allows for it to be completed.
///
/// See [The WebTransport Protocol Framework, Section 4.3]
///
/// [The WebTransport Protocol Framework, Section 4.3]: https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-overview#section-4.3
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct CreateUniStreamCommand {
    pub session_id: SessionId,
    pub send_order: SendOrder,
}

/// A send order number that, if provided, opts the created stream in to
/// participating in strict ordering.
///
/// Bytes currently queued on strictly ordered streams will be sent ahead of
/// bytes currently queued on other strictly ordered streams created with lower
/// send order numbers.
///
/// If no send order number is provided, then the order in which the user agent
/// sends bytes from it relative to other streams is implementation-defined.
/// User agents are strongly encouraged however to divide bandwidth fairly
/// between all streams that aren’t starved by lower send order numbers.
///
/// See <https://w3c.github.io/webtransport/#dom-webtransportsendoptions-sendorder>
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct SendOrder(pub(crate) u32);

/// Creates an outgoing bidirectional stream.
///
/// This operation may block until the flow control of the underlying
/// protocol allows for it to be completed.
///
/// See [The WebTransport Protocol Framework, Section 4.3]
///
/// [The WebTransport Protocol Framework, Section 4.3]: https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-overview#section-4.3
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct CreateBiStreamCommand {
    pub session_id: SessionId,
    pub send_order: SendOrder,
}

/// Add bytes into the stream send buffer.
///
/// The sender can also indicate a FIN, signalling the fact that no new data
/// will be sent on the stream.
///
/// Not applicable for incoming unidirectional streams.
///
/// See [The WebTransport Protocol Framework, Section 4.3]
///
/// [The WebTransport Protocol Framework, Section 4.3]: https://datatracker.ietf.org/doc/html/draft-ietf-webtrans-overview#section-4.3
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct SendBytesCommand {
    pub stream_id: StreamId,
    pub bytes: Bytes,
    pub finished: bool,
}

impl fmt::Debug for ConfigureClientCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ConfigureClientCommand")
            .field(&format_args!("{}", type_name_of_val(&self.0)))
            .finish()
    }
}

#[cfg(not(target_family = "wasm"))]
impl<F> From<F> for ConfigureClientCommand
where
    F: FnOnce() -> Result<Client, ConfigureClientError> + Send + Sync + 'static,
{
    fn from(f: F) -> Self {
        Self(Arc::new(std::sync::Mutex::new(Some(Box::new(f)))))
    }
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
impl<F> From<F> for ConfigureClientCommand
where
    F: FnOnce() -> Result<Client, ConfigureClientError> + 'static,
{
    fn from(f: F) -> Self {
        Self(Rc::new(RefCell::new(Some(Box::new(f)))))
    }
}

impl From<ConfigureClientCommand> for WebTransportCommand {
    fn from(command: ConfigureClientCommand) -> Self {
        Self::ConfigureClient(command)
    }
}

impl From<EstablishSessionCommand> for WebTransportCommand {
    fn from(command: EstablishSessionCommand) -> Self {
        Self::EstablishSession(command)
    }
}

impl From<TerminateSessionCommand> for WebTransportCommand {
    fn from(command: TerminateSessionCommand) -> Self {
        Self::TerminateSession(command)
    }
}

impl From<u32> for ErrorCode {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl TryFrom<String> for ErrorReason {
    type Error = BoxError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() <= Self::MAX_LEN {
            Ok(Self(value))
        } else {
            Err(BoxError::from(format!(
                "error reason string must not exceed {max} bytes",
                max = Self::MAX_LEN
            )))
        }
    }
}

impl ErrorReason {
    const MAX_LEN: usize = 1024;
}

impl From<CreateUniStreamCommand> for WebTransportCommand {
    fn from(command: CreateUniStreamCommand) -> Self {
        Self::CreateUniStream(command)
    }
}

impl TryFrom<u32> for SendOrder {
    type Error = TryFromIntError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Ok(Self(i32::try_from(value)?.try_into().unwrap()))
    }
}

impl TryFrom<i32> for SendOrder {
    type Error = TryFromIntError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(Self(u32::try_from(value)?))
    }
}

impl From<CreateBiStreamCommand> for WebTransportCommand {
    fn from(command: CreateBiStreamCommand) -> Self {
        Self::CreateBiStream(command)
    }
}

impl From<SendBytesCommand> for WebTransportCommand {
    fn from(command: SendBytesCommand) -> Self {
        Self::SendBytes(command)
    }
}
