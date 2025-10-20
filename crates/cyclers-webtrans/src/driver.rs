#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use std::rc::Rc;
#[cfg(not(target_family = "wasm"))]
use std::sync::Arc;

use cyclers::BoxError;
use cyclers::driver::Driver;
use futures_concurrency::future::TryJoin as _;
use futures_lite::{Stream, StreamExt as _, pin, stream};
use futures_rx::stream_ext::share::Shared;
use futures_rx::{PublishSubject, RxExt as _};
use papaya::HashMap;
use tokio::sync::{RwLock, mpsc};
#[cfg(feature = "tracing")]
use tracing::{debug, instrument};
#[cfg(feature = "tracing")]
use tracing_futures::Instrument as _;
#[cfg(all(
    not(target_family = "wasm"),
    any(feature = "rustls-aws-lc-rs", feature = "rustls-ring")
))]
use web_transport_quinn::ClientBuilder;
#[cfg(not(target_family = "wasm"))]
use web_transport_quinn::{Client, Session};
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use web_transport_wasm::{Client, ClientBuilder, Session};

use crate::command::{ConfigureClientCommand, SendBytesCommand, TerminateSessionCommand};
use crate::session::SessionId;
use crate::stream::StreamId;
use crate::{WebTransportCommand, WebTransportSource};

pub struct WebTransportDriver;

#[cfg(not(target_family = "wasm"))]
pub(crate) type SessionMap = Arc<HashMap<SessionId, RwLock<Session>>>;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub(crate) type SessionMap = Rc<HashMap<SessionId, RwLock<Session>>>;

#[cfg(not(target_family = "wasm"))]
pub(crate) type StreamMap = Arc<HashMap<StreamId, crate::stream::Stream>>;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub(crate) type StreamMap = Rc<HashMap<StreamId, crate::stream::Stream>>;

impl<Sink> Driver<Sink> for WebTransportDriver
where
    Sink: Stream<Item = WebTransportCommand>,
{
    type Source = WebTransportSource<Sink>;
    type Termination = ();

    fn call(
        self,
        sink: Sink,
    ) -> (
        Self::Source,
        impl Future<Output = Result<Self::Termination, BoxError>>,
    ) {
        let sink = sink.share();

        let sessions = SessionMap::default();
        let streams = StreamMap::default();

        const CLIENT_BUFFER_LEN: usize = 1;

        let (client_tx, client_rx) = mpsc::channel(CLIENT_BUFFER_LEN);

        let source =
            WebTransportSource::new(sink.clone(), sessions.clone(), streams.clone(), client_rx);

        let run = self.run(sink, sessions, streams, client_tx);

        (source, run)
    }
}

impl WebTransportDriver {
    #[cfg_attr(
        feature = "tracing",
        instrument(level = "debug", skip(self, sink, sessions, streams, client_tx))
    )]
    async fn run<Sink>(
        self,
        sink: Shared<Sink, PublishSubject<Sink::Item>>,
        sessions: SessionMap,
        streams: StreamMap,
        client_tx: mpsc::Sender<Client>,
    ) -> Result<<Self as Driver<Sink>>::Termination, BoxError>
    where
        Sink: Stream<Item = WebTransportCommand>,
    {
        let configure_client = sink.clone().filter_map(|command| {
            if let WebTransportCommand::ConfigureClient(command) = &*command {
                Some(command.clone())
            } else {
                None
            }
        });
        #[cfg(any(
            all(
                not(target_family = "wasm"),
                any(feature = "rustls-aws-lc-rs", feature = "rustls-ring")
            ),
            all(target_family = "wasm", target_os = "unknown"),
        ))]
        let establish_session = sink.clone().filter_map(|command| {
            if let WebTransportCommand::EstablishSession(command) = &*command {
                Some(command.clone())
            } else {
                None
            }
        });
        let terminate_session = sink.clone().filter_map(|command| {
            if let WebTransportCommand::TerminateSession(command) = &*command {
                Some(command.clone())
            } else {
                None
            }
        });
        let send_bytes = sink.filter_map(|command| {
            if let WebTransportCommand::SendBytes(command) = &*command {
                Some(command.clone())
            } else {
                None
            }
        });

        let configure_client = async move {
            #[cfg(any(
                all(
                    not(target_family = "wasm"),
                    any(feature = "rustls-aws-lc-rs", feature = "rustls-ring")
                ),
                all(target_family = "wasm", target_os = "unknown"),
            ))]
            let default_client = {
                // Only create the default client once we have received the first
                // `EstablishSession` command.
                let default_client = establish_session.take(1).then(|_| {
                    let client_tx = client_tx.clone();
                    async move {
                        #[cfg(all(
                            not(target_family = "wasm"),
                            any(feature = "rustls-aws-lc-rs", feature = "rustls-ring")
                        ))]
                        let client = ClientBuilder::new().with_system_roots()?;
                        #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                        let client = ClientBuilder::new().with_system_roots();

                        #[cfg(feature = "tracing")]
                        debug!(?client, "default client");

                        let permit = client_tx.reserve().await?;
                        permit.send(client);

                        Ok::<_, BoxError>(())
                    }
                });
                // Once we receive a `ConfigureClient` command, we will never yield the default
                // client.
                configure_client
                    .clone()
                    .take(1)
                    .flat_map(|_| stream::empty())
                    .fallback(default_client)
            };

            let configure_client =
                configure_client.then(|ConfigureClientCommand(configure_client_fn)| {
                    let client_tx = client_tx.clone();
                    async move {
                        #[cfg(not(target_family = "wasm"))]
                        let configure_client_fn = {
                            let mut configure_client_fn = configure_client_fn.lock().unwrap();
                            configure_client_fn
                                .take()
                                .expect("`configure_client_fn` should not be `None`")
                        };
                        #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                        let configure_client_fn = {
                            let mut configure_client_fn = configure_client_fn
                                .try_borrow_mut()
                                .expect("`configure_client_fn` should not already be borrowed");
                            configure_client_fn
                                .take()
                                .expect("`configure_client_fn` should not be `None`")
                        };

                        let client = configure_client_fn()?;

                        #[cfg(feature = "tracing")]
                        debug!(?client, "configured client");

                        let permit = client_tx.reserve().await?;
                        permit.send(client);

                        Ok::<_, BoxError>(())
                    }
                });
            #[cfg(any(
                all(
                    not(target_family = "wasm"),
                    any(feature = "rustls-aws-lc-rs", feature = "rustls-ring")
                ),
                all(target_family = "wasm", target_os = "unknown"),
            ))]
            let configure_client = configure_client.fallback(default_client.or(stream::pending()));
            pin!(configure_client);

            while configure_client.try_next().await?.is_some() {}
            Ok(())
        };

        let terminate_session = async move {
            let terminate_session = terminate_session.then(
                move |TerminateSessionCommand {
                          session_id,
                          code,
                          reason,
                      }| {
                    let sessions = sessions.clone();
                    async move {
                        #[cfg(feature = "tracing")]
                        debug!(?code, ?reason, ?session_id, "terminating session");

                        let sessions = sessions.pin_owned();

                        let session = &mut *sessions
                            .get(&session_id)
                            .ok_or_else(|| BoxError::from("invalid session ID"))?
                            .write()
                            .await;

                        #[cfg(not(target_family = "wasm"))]
                        session.close(code.0, reason.0.as_bytes());
                        #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                        session.close(code.0, &reason.0);

                        Ok::<_, BoxError>(())
                    }
                },
            );
            pin!(terminate_session);

            while terminate_session.try_next().await?.is_some() {}
            Ok(())
        };

        let send_bytes = async move {
            let streams = streams.clone();
            let send_bytes = stream::unfold(send_bytes, move |mut commands| {
                let streams = streams.clone();
                async move {
                    let SendBytesCommand {
                        stream_id,
                        bytes,
                        finished,
                    } = commands.next().await?;

                    #[cfg(feature = "tracing")]
                    debug!(?bytes, finished, ?stream_id, "sending bytes");

                    let streams = streams.pin_owned();

                    let stream = match streams.get(&stream_id) {
                        Some(stream) => stream,
                        None => {
                            return Some((Err(BoxError::from("invalid stream ID")), commands));
                        },
                    };

                    let mut tx = match stream {
                        crate::stream::Stream::OutgoingUni(tx) => tx,
                        crate::stream::Stream::OutgoingBi(tx, _) => tx,
                        crate::stream::Stream::IncomingUni(_) => {
                            return Some((
                                Err(BoxError::from("stream is not a writable stream")),
                                commands,
                            ));
                        },
                        crate::stream::Stream::IncomingBi(tx, _) => tx,
                    }
                    .write()
                    .await;

                    #[cfg(not(target_family = "wasm"))]
                    if let Err(err) = tx.write_all(&bytes).await {
                        return Some((Err(err.into()), commands));
                    }
                    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                    if let Err(err) = tx.write_buf(&mut bytes.clone()).await {
                        return Some((Err(BoxError::from(format!("{err}"))), commands));
                    }

                    if finished {
                        let _ = tx.finish();
                        None
                    } else {
                        Some((Ok(()), commands))
                    }
                }
            });
            pin!(send_bytes);

            while send_bytes.try_next().await?.is_some() {}
            Ok::<_, BoxError>(())
        };

        let run = (configure_client, terminate_session, send_bytes).try_join();

        #[cfg(feature = "tracing")]
        let run = run.in_current_span();

        run.await.map(|_| ())
    }
}
