use bytes::Bytes;
use cyclers::BoxError;
use cyclers::driver::{Driver, Source};
use futures_lite::{Stream, StreamExt as _, stream};
use futures_rx::stream_ext::share::Shared;
use futures_rx::{PublishSubject, RxExt as _};
pub use http::{Request, Response};
#[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
use http_body_util::BodyExt as _;
#[cfg(all(target_os = "wasi", target_env = "p2"))]
use wstd::http::IntoBody as _;

pub struct HttpDriver;

pub struct HttpSource<Sink>
where
    Sink: Stream,
{
    sink: Shared<Sink, PublishSubject<Sink::Item>>,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum HttpCommand {
    SendRequest(Request<Bytes>),
}

impl<Sink> Driver<Sink> for HttpDriver
where
    Sink: Stream<Item = HttpCommand>,
{
    type Input = HttpCommand;
    type Source = HttpSource<Sink>;

    fn call(self, sink: Sink) -> (Self::Source, impl Future<Output = Result<(), BoxError>>) {
        let sink = sink.share();

        (HttpSource { sink: sink.clone() }, async move {
            // Allow the driver future to exit when the "sink" stream has finished.
            let mut sink = sink;
            while sink.next().await.is_some() {}
            Ok(())
        })
    }
}

impl<Sink> Source for HttpSource<Sink> where Sink: Stream {}

#[cfg(any(
    not(any(target_family = "wasm", target_os = "wasi")),
    all(target_family = "wasm", target_os = "unknown")
))]
impl<Sink> HttpSource<Sink>
where
    Sink: Stream<Item = HttpCommand>,
{
    /// Returns a [`Stream`] that yields responses received from the server.
    pub fn response(&self) -> impl Stream<Item = Result<Response<Bytes>, BoxError>> + use<Sink> {
        let send_request = self
            .sink
            .clone()
            .filter(|command| matches!(**command, HttpCommand::SendRequest(..)));

        stream::unfold((send_request,), move |(mut send_request,)| async move {
            #[allow(irrefutable_let_patterns)]
            let HttpCommand::SendRequest(request) = &*send_request.next().await? else {
                unreachable!();
            };

            let (parts, body) = request.clone().into_parts();
            let request = Request::from_parts(parts, body);
            let request = reqwest::Request::try_from(request).expect("request should be valid");

            match reqwest::Client::new().execute(request).await {
                Ok(response) => {
                    #[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
                    {
                        let response = Response::from(response);
                        let (parts, body) = response.into_parts();
                        let body = match body.collect().await {
                            Ok(bytes) => bytes.to_bytes(),
                            Err(err) => {
                                return Some((Err(err.into()), (send_request,)));
                            },
                        };
                        let response = Response::from_parts(parts, body);

                        Some((Ok(response), (send_request,)))
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
                                return Some((Err(err.into()), (send_request,)));
                            },
                        };
                        let response = match res.body(bytes) {
                            Ok(response) => response,
                            Err(err) => {
                                return Some((Err(err.into()), (send_request,)));
                            },
                        };

                        Some((Ok(response), (send_request,)))
                    }
                },
                Err(err) => Some((Err(err.into()), (send_request,))),
            }
        })
    }
}

#[cfg(all(target_os = "wasi", target_env = "p2"))]
impl<Sink> HttpSource<Sink>
where
    Sink: Stream<Item = HttpCommand>,
{
    /// Returns a [`Stream`] that yields responses received from the server.
    pub fn response(&self) -> impl Stream<Item = Result<Response<Bytes>, BoxError>> + use<Sink> {
        let send_request = self
            .sink
            .clone()
            .filter(|command| matches!(**command, HttpCommand::SendRequest(..)));

        stream::unfold((send_request,), move |(mut send_request,)| async move {
            #[allow(irrefutable_let_patterns)]
            let HttpCommand::SendRequest(request) = &*send_request.next().await? else {
                unreachable!();
            };

            let (parts, body) = request.clone().into_parts();
            let request = Request::from_parts(parts, body.as_ref().into_body());

            match wstd::http::Client::new().send(request).await {
                Ok(response) => {
                    let (parts, mut body) = response.into_parts();
                    let body = match body.bytes().await {
                        Ok(bytes) => Bytes::from(bytes),
                        Err(err) => {
                            return Some((Err(err.into()), (send_request,)));
                        },
                    };
                    let response = Response::from_parts(parts, body);

                    Some((Ok(response), (send_request,)))
                },
                Err(err) => Some((Err(err.into()), (send_request,))),
            }
        })
    }
}
