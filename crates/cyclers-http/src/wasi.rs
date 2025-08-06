use wstd::http::Client;
use wstd::time::Duration;

/// A `ClientBuilder` can be used to create a `Client` with custom
/// configuration.
#[derive(Debug, Default)]
pub struct ClientBuilder {
    options: RequestOptions,
}

#[derive(Debug, Default)]
struct RequestOptions {
    connect_timeout: Option<Duration>,
    first_byte_timeout: Option<Duration>,
    between_bytes_timeout: Option<Duration>,
}

impl ClientBuilder {
    /// Constructs a new `ClientBuilder`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a `Client` that uses this `ClientBuilder` configuration.
    pub fn build(self) -> Client {
        let mut client = Client::new();

        if let Some(connect_timeout) = self.options.connect_timeout {
            client.set_connect_timeout(connect_timeout);
        }
        if let Some(first_byte_timeout) = self.options.first_byte_timeout {
            client.set_first_byte_timeout(first_byte_timeout);
        }
        if let Some(between_bytes_timeout) = self.options.between_bytes_timeout {
            client.set_between_bytes_timeout(between_bytes_timeout);
        }

        client
    }

    /// Sets a timeout for connecting to the HTTP server.
    pub fn connect_timeout(mut self, timeout: impl Into<Duration>) -> Self {
        self.options.connect_timeout = Some(timeout.into());
        self
    }

    /// Sets a timeout for receiving the first byte of the Response body.
    pub fn first_byte_timeout(mut self, timeout: impl Into<Duration>) -> Self {
        self.options.first_byte_timeout = Some(timeout.into());
        self
    }

    /// Sets a timeout for receiving subsequent chunks of bytes in the Response
    /// body stream.
    pub fn between_bytes_timeout(mut self, timeout: impl Into<Duration>) -> Self {
        self.options.between_bytes_timeout = Some(timeout.into());
        self
    }
}
