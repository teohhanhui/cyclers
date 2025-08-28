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
use tokio::sync::RwLock;
#[cfg(feature = "tracing")]
use tracing::{debug, instrument};
#[cfg(feature = "tracing")]
use tracing_futures::Instrument as _;
#[cfg(not(target_family = "wasm"))]
use web_transport_quinn::Session;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use web_transport_wasm::Session;

use crate::command::{SendBytesCommand, TerminateSessionCommand};
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

        let source = WebTransportSource::new(sink.clone(), sessions.clone(), streams.clone());

        let run = self.run(sink, sessions, streams);

        (source, run)
    }
}

impl WebTransportDriver {
    #[cfg_attr(
        feature = "tracing",
        instrument(level = "debug", skip(self, sink, sessions, streams))
    )]
    async fn run<Sink>(
        self,
        sink: Shared<Sink, PublishSubject<Sink::Item>>,
        sessions: SessionMap,
        streams: StreamMap,
    ) -> Result<<Self as Driver<Sink>>::Termination, BoxError>
    where
        Sink: Stream<Item = WebTransportCommand>,
    {
        let terminate_session_commands = sink.clone().filter_map(move |command| {
            if let WebTransportCommand::TerminateSession(command) = &*command {
                Some(command.clone())
            } else {
                None
            }
        });
        let send_bytes_commands = sink.filter_map(move |command| {
            if let WebTransportCommand::SendBytes(command) = &*command {
                Some(command.clone())
            } else {
                None
            }
        });

        let do_terminate_session = async move {
            let do_terminate_session = terminate_session_commands.then(
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
            pin!(do_terminate_session);

            while do_terminate_session.try_next().await?.is_some() {}
            Ok(())
        };

        let do_send_bytes = async move {
            let streams = streams.clone();
            let do_send_bytes = stream::unfold(send_bytes_commands, move |mut commands| {
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
                        tx.finish().ok();
                        None
                    } else {
                        Some((Ok(()), commands))
                    }
                }
            });
            pin!(do_send_bytes);

            while do_send_bytes.try_next().await?.is_some() {}
            Ok::<_, BoxError>(())
        };

        let run = (do_terminate_session, do_send_bytes).try_join();

        #[cfg(feature = "tracing")]
        let run = run.in_current_span();

        run.await.map(|_| ())
    }
}
