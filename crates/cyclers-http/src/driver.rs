#[cfg(all(target_os = "wasi", target_env = "p2"))]
use std::convert::Infallible;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

use bytes::Bytes;
use cyclers::BoxError;
use cyclers::driver::{Driver, Source};
use futures_concurrency::future::TryJoin as _;
use futures_concurrency::stream::Zip as _;
use futures_lite::{Stream, StreamExt as _, pin, stream};
use futures_rx::stream_ext::share::Shared;
use futures_rx::{PublishSubject, RxExt as _};
pub use http::{Request, Response};
#[cfg(not(target_family = "wasm"))]
use http_body_util::BodyExt as _;
#[cfg(any(
    not(target_family = "wasm"),
    all(target_family = "wasm", target_os = "unknown")
))]
use reqwest::Client;
#[cfg(any(
    not(target_family = "wasm"),
    all(target_family = "wasm", target_os = "unknown")
))]
pub use reqwest::ClientBuilder;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
#[cfg(feature = "tracing")]
use tracing::{debug, instrument};
#[cfg(feature = "tracing")]
use tracing_futures::Instrument as _;
#[cfg(all(target_os = "wasi", target_env = "p2"))]
use wstd::http::Client;
#[cfg(all(target_os = "wasi", target_env = "p2"))]
use wstd::http::IntoBody as _;

#[cfg(all(target_os = "wasi", target_env = "p2"))]
pub use crate::wasi::ClientBuilder;

const CLIENT_BUFFER_LEN: usize = 1;

/// Type alias for a `ClientBuilder` that can be cloned.
type ArcClientBuilder = Arc<std::sync::Mutex<Option<ClientBuilder>>>;

/// Type alias for a boxed `Request`.
type BoxRequest = Box<Request<Bytes>>;

/// Type alias for a `Client` that can be cloned.
#[cfg(any(
    not(target_family = "wasm"),
    all(target_family = "wasm", target_os = "unknown")
))]
type ArcClient = Client;
/// Type alias for a `Client` that can be cloned.
#[cfg(all(target_os = "wasi", target_env = "p2"))]
type ArcClient = Arc<Client>;

pub struct HttpDriver;

pub struct HttpSource<Sink>
where
    Sink: Stream,
{
    sink: Shared<Sink, PublishSubject<Sink::Item>>,
    client_tx: mpsc::Sender<ArcClient>,
    client: Shared<stream::Boxed<ArcClient>, PublishSubject<ArcClient>>,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum HttpCommand {
    ConfigureClient(ArcClientBuilder),
    SendRequest(BoxRequest),
}

/// The error type returned when [`HttpCommand::ConfigureClient`] fails.
#[derive(Debug)]
#[non_exhaustive]
pub struct ConfigureClientError {
    kind: ConfigureClientErrorKind,
    #[cfg_attr(all(target_os = "wasi", target_env = "p2"), allow(dead_code))]
    inner: BoxError,
}

/// The various types of errors that can cause [`HttpCommand::ConfigureClient`]
/// to fail.
#[derive(Debug)]
#[non_exhaustive]
pub enum ConfigureClientErrorKind {
    /// Failed to create `Client` from `ClientBuilder`.
    #[cfg(any(
        not(target_family = "wasm"),
        all(target_family = "wasm", target_os = "unknown")
    ))]
    Builder,
}

impl<Sink> Driver<Sink> for HttpDriver
where
    Sink: Stream<Item = HttpCommand>,
{
    type Input = HttpCommand;
    type Source = HttpSource<Sink>;
    type Termination = ();

    fn call(
        self,
        sink: Sink,
    ) -> (
        Self::Source,
        impl Future<Output = Result<Self::Termination, BoxError>>,
    ) {
        let sink = sink.share();

        let (client_tx, client_rx) = mpsc::channel(CLIENT_BUFFER_LEN);

        (
            HttpSource::new(sink.clone(), client_tx.clone(), client_rx),
            self.run(sink, client_tx),
        )
    }
}

impl HttpDriver {
    #[cfg_attr(
        feature = "tracing",
        instrument(level = "debug", skip(self, sink, client_tx))
    )]
    async fn run<Sink>(
        self,
        sink: Shared<Sink, PublishSubject<Sink::Item>>,
        client_tx: mpsc::Sender<ArcClient>,
    ) -> Result<<Self as Driver<Sink>>::Termination, BoxError>
    where
        Sink: Stream<Item = HttpCommand>,
    {
        let configure_client =
            sink.filter(|command| matches!(**command, HttpCommand::ConfigureClient(..)));

        let run = (async move {
            let s = stream::unfold(
                (client_tx, configure_client),
                |(client_tx, mut configure_client)| async move {
                    let config_client = configure_client.next().await?;

                    let HttpCommand::ConfigureClient(client_builder) = &*config_client else {
                        unreachable!();
                    };

                    let client_builder = {
                        let mut client_builder = client_builder.lock().unwrap();
                        client_builder
                            .take()
                            .expect("`client_builder` should not be `None`")
                    };

                    #[cfg(feature = "tracing")]
                    debug!(?client_builder, "configuring client");

                    #[cfg(any(
                        not(target_family = "wasm"),
                        all(target_family = "wasm", target_os = "unknown")
                    ))]
                    {
                        // TODO: This might block the current thread in the async executor.
                        // See <https://github.com/seanmonstar/reqwest/issues/2437>
                        match client_builder.build() {
                            Ok(client) => {
                                let permit = client_tx.reserve().await.ok()?;
                                permit.send(client);

                                Some((Ok(()), (client_tx, configure_client)))
                            },
                            Err(err) => Some((
                                Err(ConfigureClientError {
                                    kind: ConfigureClientErrorKind::Builder,
                                    inner: err.into(),
                                }),
                                (client_tx, configure_client),
                            )),
                        }
                    }
                    #[cfg(all(target_os = "wasi", target_env = "p2"))]
                    {
                        let client = Arc::new(client_builder.build());

                        let permit = client_tx.reserve().await.ok()?;
                        permit.send(client);

                        Some((Ok::<_, Infallible>(()), (client_tx, configure_client)))
                    }
                },
            );
            pin!(s);

            while s.try_next().await?.is_some() {}
            Ok(())
        },)
            .try_join();

        #[cfg(feature = "tracing")]
        let run = run.in_current_span();

        run.await.map(|_| ())
    }
}

impl<Sink> Source for HttpSource<Sink> where Sink: Stream {}

impl<Sink> HttpSource<Sink>
where
    Sink: Stream<Item = HttpCommand>,
{
    fn new(
        sink: Shared<Sink, PublishSubject<Sink::Item>>,
        client_tx: mpsc::Sender<ArcClient>,
        client_rx: mpsc::Receiver<ArcClient>,
    ) -> Self {
        Self {
            sink,
            client_tx,
            client: ReceiverStream::new(client_rx)
                .flat_map(stream::repeat)
                .boxed()
                .share(),
        }
    }
}

#[cfg(any(
    not(target_family = "wasm"),
    all(target_family = "wasm", target_os = "unknown")
))]
impl<Sink> HttpSource<Sink>
where
    Sink: Stream<Item = HttpCommand>,
{
    /// Returns a [`Stream`] that yields responses received from the server.
    #[cfg_attr(feature = "tracing", instrument(level = "debug", skip(self)))]
    pub fn response(&self) -> impl Stream<Item = Result<Response<Bytes>, BoxError>> + use<Sink> {
        let client = self.client.clone().or(stream::once_future({
            let client_tx = self.client_tx.clone();
            async move {
                let client = Client::new();

                #[cfg(feature = "tracing")]
                debug!(?client, "using default client");

                if let Ok(permit) = client_tx.reserve().await {
                    permit.send(client.clone());
                }

                client
            }
        })
        .share());

        let send_request = self
            .sink
            .clone()
            .filter(|command| matches!(**command, HttpCommand::SendRequest(..)));

        let response = stream::unfold((client, send_request).zip(), move |mut s| async move {
            let (client, send_request) = s.next().await?;

            #[allow(irrefutable_let_patterns)]
            let HttpCommand::SendRequest(request) = &*send_request else {
                unreachable!();
            };

            #[cfg(feature = "tracing")]
            debug!(?request, "sending request");

            let (parts, body) = request.clone().into_parts();
            let request = Request::from_parts(parts, body);
            let request = reqwest::Request::try_from(request).expect("`request` should be valid");

            match client.execute(request).await {
                Ok(response) => {
                    #[cfg(feature = "tracing")]
                    debug!(?response, "received response");

                    #[cfg(not(target_family = "wasm"))]
                    {
                        let response = Response::from(response);
                        let (parts, body) = response.into_parts();
                        let body = match body.collect().await {
                            Ok(bytes) => bytes.to_bytes(),
                            Err(err) => {
                                return Some((Err(err.into()), s));
                            },
                        };
                        let response = Response::from_parts(parts, body);

                        Some((Ok(response), s))
                    }
                    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                    {
                        let mut res = Response::builder()
                            .status(response.status().as_str())
                            // .version(unimplemented!())
                        ;
                        {
                            let headers = res.headers_mut().unwrap();
                            headers.extend(
                                response
                                    .headers()
                                    .iter()
                                    .map(|(k, v)| (k.clone(), v.clone())),
                            );
                        }
                        let bytes = match response.bytes().await {
                            Ok(bytes) => bytes,
                            Err(err) => {
                                return Some((Err(err.into()), s));
                            },
                        };
                        let response = match res.body(bytes) {
                            Ok(response) => response,
                            Err(err) => {
                                return Some((Err(err.into()), s));
                            },
                        };

                        Some((Ok(response), s))
                    }
                },
                Err(err) => Some((Err(err.into()), s)),
            }
        });

        #[cfg(feature = "tracing")]
        let response = response.in_current_span();

        response
    }
}

#[cfg(all(target_os = "wasi", target_env = "p2"))]
impl<Sink> HttpSource<Sink>
where
    Sink: Stream<Item = HttpCommand>,
{
    /// Returns a [`Stream`] that yields responses received from the server.
    #[cfg_attr(feature = "tracing", instrument(level = "debug", skip(self)))]
    pub fn response(&self) -> impl Stream<Item = Result<Response<Bytes>, BoxError>> + use<Sink> {
        let client = self.client.clone().or(stream::once_future({
            let client_tx = self.client_tx.clone();
            async move {
                let client = Arc::new(Client::new());

                #[cfg(feature = "tracing")]
                debug!(?client, "using default client");

                if let Ok(permit) = client_tx.reserve().await {
                    permit.send(Arc::clone(&client));
                }

                client
            }
        })
        .share());

        let send_request = self
            .sink
            .clone()
            .filter(|command| matches!(**command, HttpCommand::SendRequest(..)));

        let response = stream::unfold((client, send_request).zip(), move |mut s| async move {
            let (client, send_request) = s.next().await?;

            #[allow(irrefutable_let_patterns)]
            let HttpCommand::SendRequest(request) = &*send_request else {
                unreachable!();
            };

            #[cfg(feature = "tracing")]
            debug!(?request, "sending request");

            let (parts, body) = request.clone().into_parts();
            let request = Request::from_parts(parts, body.as_ref().into_body());

            match client.send(request).await {
                Ok(response) => {
                    #[cfg(feature = "tracing")]
                    debug!(?response, "received response");

                    let (parts, mut body) = response.into_parts();
                    let body = match body.bytes().await {
                        Ok(bytes) => Bytes::from(bytes),
                        Err(err) => {
                            return Some((Err(err.into()), s));
                        },
                    };
                    let response = Response::from_parts(parts, body);

                    Some((Ok(response), s))
                },
                Err(err) => Some((Err(err.into()), s)),
            }
        });

        #[cfg(feature = "tracing")]
        let response = response.in_current_span();

        response
    }
}

impl From<Request<Bytes>> for HttpCommand {
    fn from(request: Request<Bytes>) -> Self {
        Self::SendRequest(Box::new(request))
    }
}

impl From<ClientBuilder> for HttpCommand {
    fn from(client_builder: ClientBuilder) -> Self {
        Self::ConfigureClient(Arc::new(std::sync::Mutex::new(Some(client_builder))))
    }
}

impl fmt::Display for ConfigureClientError {
    fn fmt(
        &self,
        #[cfg_attr(all(target_os = "wasi", target_env = "p2"), allow(unused_variables))]
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self.kind {
            #[cfg(any(
                not(target_family = "wasm"),
                all(target_family = "wasm", target_os = "unknown")
            ))]
            ConfigureClientErrorKind::Builder => {
                let err = self.inner.downcast_ref::<reqwest::Error>().unwrap();
                write!(f, "failed to create Client from ClientBuilder: {err}")
            },
        }
    }
}

impl Error for ConfigureClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.kind {
            #[cfg(any(
                not(target_family = "wasm"),
                all(target_family = "wasm", target_os = "unknown")
            ))]
            ConfigureClientErrorKind::Builder => {
                let err = self.inner.downcast_ref::<reqwest::Error>().unwrap();
                Some(err)
            },
        }
    }
}

impl ConfigureClientError {
    /// Returns the corresponding [`ConfigureClientErrorKind`] for this error.
    #[must_use]
    pub const fn kind(&self) -> &ConfigureClientErrorKind {
        &self.kind
    }
}
