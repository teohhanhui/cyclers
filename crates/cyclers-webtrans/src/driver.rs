use std::sync::Arc;

use cyclers::BoxError;
use cyclers::driver::{Driver, Source};
use futures_concurrency::future::TryJoin as _;
use futures_lite::{Stream, StreamExt as _, pin, stream};
use futures_rx::stream_ext::share::Shared;
use futures_rx::{PublishSubject, RxExt as _};
use papaya::HashMap;
use tokio::sync::RwLock;
#[cfg(feature = "tracing")]
use tracing::{debug, instrument};
#[cfg(feature = "tracing")]
use tracing_futures::Instrument as _;
use url::Url;
use web_transport::{RecvStream, SendStream, Session};

pub struct WebTransportDriver;

pub struct WebTransportSource {
    sessions: Arc<HashMap<SessionId, RwLock<Session>>>,
    streams: Arc<HashMap<StreamId, RwLock<(SendStream, RecvStream)>>>,
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[non_exhaustive]
pub enum WebTransportCommand {
    EstablishSession {
        url: Url,
    },
    TerminateSession {
        session_id: SessionId,
        code: u32,
        reason: String,
    },
    /// Creates an outgoing unidirectional stream.
    ///
    /// This operation may block until the flow control of the underlying
    /// protocol allows for it to be completed.
    CreateUniStream(SessionId),
    /// Creates an outgoing bidirectional stream.
    ///
    /// This operation may block until the flow control of the underlying
    /// protocol allows for it to be completed.
    CreateBiStream(SessionId),
    /// Add bytes into the stream send buffer.
    ///
    /// The sender can also indicate a FIN, signalling the fact that no new data
    /// will be sent on the stream.
    ///
    /// Not applicable for incoming unidirectional streams.
    SendBytes(StreamId, Bytes),
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct SessionId(usize);

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct StreamId(usize);

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Bytes {
    Bytes(bytes::Bytes),
    /// FIN, signalling the fact that no new data will be sent.
    Fin,
}

impl<Sink> Driver<Sink> for WebTransportDriver
where
    Sink: Stream<Item = WebTransportCommand>,
{
    type Source = WebTransportSource;
    type Termination = ();

    fn call(
        self,
        sink: Sink,
    ) -> (
        Self::Source,
        impl Future<Output = Result<Self::Termination, BoxError>>,
    ) {
        let sink = sink.share();

        let sessions = Arc::new(HashMap::new());
        let streams = Arc::new(HashMap::new());

        (
            WebTransportSource {
                sessions: Arc::clone(&sessions),
                streams: Arc::clone(&streams),
            },
            self.run(sink, sessions, streams),
        )
    }
}

impl WebTransportDriver {
    #[cfg_attr(
        feature = "tracing",
        instrument(level = "debug", skip(self, sink, sessions))
    )]
    async fn run<Sink>(
        self,
        sink: Shared<Sink, PublishSubject<Sink::Item>>,
        sessions: Arc<HashMap<SessionId, RwLock<Session>>>,
        streams: Arc<HashMap<StreamId, RwLock<(SendStream, RecvStream)>>>,
    ) -> Result<<Self as Driver<Sink>>::Termination, BoxError>
    where
        Sink: Stream<Item = WebTransportCommand>,
    {
        let disconnect = sink
            .clone()
            .filter(|command| matches!(**command, WebTransportCommand::TerminateSession { .. }));
        let send = sink.filter(|command| matches!(**command, WebTransportCommand::SendBytes(..)));

        let run = (
            {
                let streams = Arc::clone(&streams);
                async move {
                    let s = stream::unfold(send.clone(), |mut s| {
                        let streams = Arc::clone(&streams);
                        async move {
                            let send_bytes = s.next().await?;

                            let WebTransportCommand::SendBytes(stream_id, bytes) = &*send_bytes
                            else {
                                unreachable!();
                            };

                            #[cfg(feature = "tracing")]
                            debug!(?bytes, ?stream_id, "sending bytes");

                            let streams = streams.pin_owned();

                            let (tx, _) = &mut *(match streams.get(stream_id) {
                                Some(stream) => stream,
                                None => return Some((Err(BoxError::from("invalid stream ID")), s)),
                            })
                            .write()
                            .await;

                            let mut bytes = match bytes {
                                Bytes::Bytes(bytes) => bytes.clone(),
                                Bytes::Fin => {
                                    tx.finish().ok();
                                    return None;
                                },
                            };

                            Some((
                                // TODO: This method is not cancellation safe.
                                tx.write_buf(&mut bytes).await.map_err(|err| {
                                    #[cfg(not(target_family = "wasm"))]
                                    {
                                        BoxError::from(err)
                                    }
                                    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                                    {
                                        BoxError::from(format!("{err}"))
                                    }
                                }),
                                s,
                            ))
                        }
                    });
                    pin!(s);

                    while s.try_next().await?.is_some() {}
                    Ok(())
                }
            },
            async move {
                let s = disconnect.clone().then(move |terminate_session| {
                    let sessions = Arc::clone(&sessions);
                    async move {
                        let WebTransportCommand::TerminateSession {
                            session_id,
                            code,
                            reason,
                        } = &*terminate_session
                        else {
                            unreachable!();
                        };

                        #[cfg(feature = "tracing")]
                        debug!("terminating session");

                        let sessions = sessions.pin_owned();

                        let session = &mut *sessions
                            .get(session_id)
                            .ok_or_else(|| BoxError::from("invalid session ID"))?
                            .write()
                            .await;

                        session.close(*code, reason);

                        Ok::<_, BoxError>(())
                    }
                });
                pin!(s);

                while s.try_next().await?.is_some() {}
                Ok(())
            },
        )
            .try_join();

        #[cfg(feature = "tracing")]
        let run = run.in_current_span();

        run.await.map(|_| ())
    }
}

impl Source for WebTransportSource {}
