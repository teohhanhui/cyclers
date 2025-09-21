#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use std::rc::Rc;

use bytes::{Bytes, BytesMut};
use cyclers::BoxError;
use cyclers::driver::Source;
use futures_lite::{Stream, StreamExt as _, stream};
use futures_rx::PublishSubject;
use futures_rx::stream_ext::share::Shared;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use js_sys::Symbol;
use tokio::sync::RwLock;
#[cfg(feature = "tracing")]
use tracing::{debug, instrument, trace};
#[cfg(feature = "tracing")]
use tracing_futures::Instrument as _;
#[cfg(any(
    all(
        not(target_family = "wasm"),
        any(feature = "rustls-aws-lc-rs", feature = "rustls-ring")
    ),
    all(target_family = "wasm", target_os = "unknown"),
))]
use url::Url;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use wasm_bindgen::JsValue;
#[cfg(all(
    not(target_family = "wasm"),
    any(feature = "rustls-aws-lc-rs", feature = "rustls-ring")
))]
use web_transport_quinn::ClientBuilder;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use web_transport_wasm::ClientBuilder;

use crate::WebTransportCommand;
#[cfg(any(
    all(
        not(target_family = "wasm"),
        any(feature = "rustls-aws-lc-rs", feature = "rustls-ring")
    ),
    all(target_family = "wasm", target_os = "unknown"),
))]
use crate::command::EstablishSessionCommand;
use crate::command::{CreateBiStreamCommand, CreateUniStreamCommand};
use crate::driver::{SessionMap, StreamMap};
#[cfg(any(
    all(
        not(target_family = "wasm"),
        any(feature = "rustls-aws-lc-rs", feature = "rustls-ring")
    ),
    all(target_family = "wasm", target_os = "unknown"),
))]
use crate::session::EstablishSessionError;
use crate::session::SessionId;
use crate::stream::{
    CreateStreamError, CreateStreamErrorKind, ReceiveBytesError, ReceiveBytesErrorKind,
    ReceiveStreamError, ReceiveStreamErrorKind, StreamId,
};

pub struct WebTransportSource<Sink>
where
    Sink: Stream,
{
    sink: Shared<Sink, PublishSubject<Sink::Item>>,
    sessions: SessionMap,
    streams: StreamMap,
}

impl<Sink> Clone for WebTransportSource<Sink>
where
    Sink: Stream,
{
    fn clone(&self) -> Self {
        let WebTransportSource {
            sink,
            sessions,
            streams,
        } = self;

        WebTransportSource {
            sink: sink.clone(),
            sessions: sessions.clone(),
            streams: streams.clone(),
        }
    }
}

impl<Sink> Source for WebTransportSource<Sink> where Sink: Stream {}

impl<Sink> WebTransportSource<Sink>
where
    Sink: Stream<Item = WebTransportCommand>,
{
    pub(crate) fn new(
        sink: Shared<Sink, PublishSubject<Sink::Item>>,
        sessions: SessionMap,
        streams: StreamMap,
    ) -> Self {
        Self {
            sink,
            sessions,
            streams,
        }
    }

    #[cfg(any(
        all(
            not(target_family = "wasm"),
            any(feature = "rustls-aws-lc-rs", feature = "rustls-ring")
        ),
        all(target_family = "wasm", target_os = "unknown"),
    ))]
    /// Returns a [`Stream`] that yields whenever a WebTransport session is
    /// [ready].
    ///
    /// [ready]: https://w3c.github.io/webtransport/#dom-webtransport-ready-slot
    #[cfg_attr(feature = "tracing", instrument(level = "debug", skip(self)))]
    pub fn session_ready(
        &self,
    ) -> impl Stream<Item = Result<(SessionId, Url), EstablishSessionError>> + use<Sink> {
        let establish_session_commands = self.sink.clone().filter_map(move |command| {
            if let WebTransportCommand::EstablishSession(command) = &*command {
                Some(command.clone())
            } else {
                None
            }
        });

        let sessions = self.sessions.clone();

        let session_ready =
            establish_session_commands.then(move |EstablishSessionCommand { url }| {
                let sessions = sessions.clone();
                async move {
                    #[cfg(feature = "tracing")]
                    debug!(?url, "establishing session");

                    let (session, session_id) = {
                        #[cfg(not(target_family = "wasm"))]
                        {
                            let session = ClientBuilder::new()
                                .with_system_roots()
                                .map_err(EstablishSessionError)?
                                .connect(url.clone())
                                .await
                                .map_err(EstablishSessionError)?;

                            let session_id = SessionId(session.stable_id());

                            (session, session_id)
                        }
                        #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                        {
                            let session = ClientBuilder::new()
                                .with_system_roots()
                                .connect(url.clone())
                                .await
                                .map_err(EstablishSessionError)?;

                            let session_id =
                                SessionId(Rc::new(Symbol::from(JsValue::symbol(None))));

                            (session, session_id)
                        }
                    };

                    sessions.pin().insert(
                        {
                            #[cfg(not(target_family = "wasm"))]
                            {
                                session_id
                            }
                            #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                            {
                                session_id.clone()
                            }
                        },
                        RwLock::new(session),
                    );

                    Ok((session_id, url.clone()))
                }
            });

        #[cfg(feature = "tracing")]
        let session_ready = session_ready
            .inspect(|session_ready| {
                if let Ok((session_id, url)) = session_ready {
                    debug!(?session_id, ?url, "established session");
                }
            })
            .in_current_span();

        session_ready
    }

    /// Returns a [`Stream`] that yields whenever an outgoing unidirectional
    /// stream is created.
    #[cfg_attr(feature = "tracing", instrument(level = "debug", skip(self)))]
    pub fn uni_stream_created(
        &self,
        session_id: SessionId,
    ) -> impl Stream<Item = Result<StreamId, CreateStreamError>> + use<Sink> {
        let create_uni_stream_commands = self.sink.clone().filter_map(move |command| {
            if let WebTransportCommand::CreateUniStream(command) = &*command {
                if command.session_id == session_id {
                    Some(command.clone())
                } else {
                    None
                }
            } else {
                None
            }
        });

        let sessions = self.sessions.clone();
        let streams = self.streams.clone();

        let uni_stream_created = create_uni_stream_commands.then(
            move |CreateUniStreamCommand {
                      session_id,
                      send_order,
                  }| {
                let sessions = sessions.clone();
                let streams = streams.clone();
                async move {
                    #[cfg(feature = "tracing")]
                    debug!(
                        ?send_order,
                        ?session_id,
                        "creating outgoing unidirectional stream"
                    );

                    let sessions = sessions.pin_owned();

                    let Some(session) = sessions.get(&session_id) else {
                        return Err(CreateStreamError {
                            kind: CreateStreamErrorKind::SessionNotFound,
                            inner: BoxError::from("could not find session"),
                        });
                    };
                    #[cfg(not(target_family = "wasm"))]
                    let session = session.read().await;
                    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                    let mut session = session.write().await;

                    let tx = session.open_uni().await.map_err(|err| CreateStreamError {
                        kind: CreateStreamErrorKind::InvalidTransportState,
                        inner: err.into(),
                    })?;
                    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                    let mut tx = tx;
                    if send_order.0 > 0 {
                        let send_order =
                            i32::try_from(send_order.0).expect("`send_order` should fit in `i32`");
                        #[cfg(not(target_family = "wasm"))]
                        tx.set_priority(send_order)
                            .expect("stream should not be closed");
                        #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                        tx.set_priority(send_order)
                    }

                    #[cfg(not(target_family = "wasm"))]
                    let stream_id = StreamId(tx.quic_id());
                    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                    let stream_id = StreamId(Rc::new(Symbol::from(JsValue::symbol(None))));

                    streams.pin().insert(
                        stream_id.clone(),
                        crate::stream::Stream::OutgoingUni(RwLock::new(tx)),
                    );

                    Ok(stream_id)
                }
            },
        );

        #[cfg(feature = "tracing")]
        let uni_stream_created = uni_stream_created
            .inspect(|uni_stream_created| {
                if let Ok(stream_id) = uni_stream_created {
                    debug!(?stream_id, "created outgoing unidirectional stream");
                }
            })
            .in_current_span();

        uni_stream_created
    }

    /// Returns a [`Stream`] that yields whenever an outgoing bidirectional
    /// stream is created.
    #[cfg_attr(feature = "tracing", instrument(level = "debug", skip(self)))]
    pub fn bi_stream_created(
        &self,
        session_id: SessionId,
    ) -> impl Stream<Item = Result<StreamId, CreateStreamError>> + use<Sink> {
        let create_bi_stream_commands = self.sink.clone().filter_map(move |command| {
            if let WebTransportCommand::CreateBiStream(command) = &*command {
                if command.session_id == session_id {
                    Some(command.clone())
                } else {
                    None
                }
            } else {
                None
            }
        });

        let sessions = self.sessions.clone();
        let streams = self.streams.clone();

        let bi_stream_created = create_bi_stream_commands.then(
            move |CreateBiStreamCommand {
                      session_id,
                      send_order,
                  }| {
                let sessions = sessions.clone();
                let streams = streams.clone();
                async move {
                    #[cfg(feature = "tracing")]
                    debug!(
                        ?send_order,
                        ?session_id,
                        "creating outgoing bidirectional stream"
                    );

                    let sessions = sessions.pin_owned();

                    let Some(session) = sessions.get(&session_id) else {
                        return Err(CreateStreamError {
                            kind: CreateStreamErrorKind::SessionNotFound,
                            inner: BoxError::from("could not find session"),
                        });
                    };
                    #[cfg(not(target_family = "wasm"))]
                    let session = session.read().await;
                    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                    let mut session = session.write().await;

                    let (tx, rx) = session.open_bi().await.map_err(|err| CreateStreamError {
                        kind: CreateStreamErrorKind::InvalidTransportState,
                        inner: err.into(),
                    })?;
                    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                    let mut tx = tx;
                    if send_order.0 > 0 {
                        let send_order =
                            i32::try_from(send_order.0).expect("`send_order` should fit in `i32`");
                        #[cfg(not(target_family = "wasm"))]
                        tx.set_priority(send_order)
                            .expect("stream should not be closed");
                        #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                        tx.set_priority(send_order)
                    }

                    #[cfg(not(target_family = "wasm"))]
                    let stream_id = StreamId(tx.quic_id());
                    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                    let stream_id = StreamId(Rc::new(Symbol::from(JsValue::symbol(None))));

                    streams.pin().insert(
                        stream_id.clone(),
                        crate::stream::Stream::OutgoingBi(RwLock::new(tx), RwLock::new(rx)),
                    );

                    Ok(stream_id)
                }
            },
        );

        #[cfg(feature = "tracing")]
        let bi_stream_created = bi_stream_created
            .inspect(|bi_stream_created| {
                if let Ok(stream_id) = bi_stream_created {
                    debug!(?stream_id, "created outgoing bidirectional stream");
                }
            })
            .in_current_span();

        bi_stream_created
    }

    /// Returns a [`Stream`] that yields whenever an incoming unidirectional
    /// stream is received.
    #[cfg_attr(feature = "tracing", instrument(level = "debug", skip(self)))]
    pub fn uni_stream_received(
        &self,
        session_id: SessionId,
    ) -> impl Stream<Item = Result<StreamId, ReceiveStreamError>> + use<Sink> {
        let sessions = self.sessions.clone();
        let streams = self.streams.clone();

        let uni_stream_received = stream::unfold((), move |_| {
            let sessions = sessions.clone();
            let streams = streams.clone();
            #[cfg(all(target_family = "wasm", target_os = "unknown"))]
            let session_id = session_id.clone();
            async move {
                let sessions = sessions.pin_owned();

                let Some(session) = sessions.get(&session_id) else {
                    return Some((
                        Err(ReceiveStreamError {
                            kind: ReceiveStreamErrorKind::SessionNotFound,
                            inner: BoxError::from("could not find session"),
                        }),
                        (),
                    ));
                };
                #[cfg(not(target_family = "wasm"))]
                let session = session.read().await;
                #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                let mut session = session.write().await;

                let rx = match session.accept_uni().await {
                    Ok(rx) => rx,
                    Err(err) => {
                        return Some((
                            Err(ReceiveStreamError {
                                kind: ReceiveStreamErrorKind::InvalidTransportState,
                                inner: err.into(),
                            }),
                            (),
                        ));
                    },
                };

                #[cfg(not(target_family = "wasm"))]
                let stream_id = StreamId(rx.quic_id());
                #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                let stream_id = StreamId(Rc::new(Symbol::from(JsValue::symbol(None))));

                streams.pin().insert(
                    stream_id.clone(),
                    crate::stream::Stream::IncomingUni(RwLock::new(rx)),
                );

                Some((Ok(stream_id), ()))
            }
        });

        #[cfg(feature = "tracing")]
        let uni_stream_received = uni_stream_received
            .inspect(|uni_stream_received| {
                if let Ok(stream_id) = uni_stream_received {
                    debug!(?stream_id, "received incoming unidirectional stream");
                }
            })
            .in_current_span();

        uni_stream_received
    }

    /// Returns a [`Stream`] that yields whenever an incoming bidirectional
    /// stream is received.
    #[cfg_attr(feature = "tracing", instrument(level = "debug", skip(self)))]
    pub fn bi_stream_received(
        &self,
        session_id: SessionId,
    ) -> impl Stream<Item = Result<StreamId, ReceiveStreamError>> + use<Sink> {
        let sessions = self.sessions.clone();
        let streams = self.streams.clone();

        let bi_stream_received = stream::unfold((), move |_| {
            let sessions = sessions.clone();
            let streams = streams.clone();
            #[cfg(all(target_family = "wasm", target_os = "unknown"))]
            let session_id = session_id.clone();
            async move {
                let sessions = sessions.pin_owned();

                let Some(session) = sessions.get(&session_id) else {
                    return Some((
                        Err(ReceiveStreamError {
                            kind: ReceiveStreamErrorKind::SessionNotFound,
                            inner: BoxError::from("could not find session"),
                        }),
                        (),
                    ));
                };
                #[cfg(not(target_family = "wasm"))]
                let session = session.read().await;
                #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                let mut session = session.write().await;

                let (tx, rx) = match session.accept_bi().await {
                    Ok((tx, rx)) => (tx, rx),
                    Err(err) => {
                        return Some((
                            Err(ReceiveStreamError {
                                kind: ReceiveStreamErrorKind::InvalidTransportState,
                                inner: err.into(),
                            }),
                            (),
                        ));
                    },
                };

                #[cfg(not(target_family = "wasm"))]
                let stream_id = StreamId(tx.quic_id());
                #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                let stream_id = StreamId(Rc::new(Symbol::from(JsValue::symbol(None))));

                streams.pin().insert(
                    stream_id.clone(),
                    crate::stream::Stream::IncomingBi(RwLock::new(tx), RwLock::new(rx)),
                );

                Some((Ok(stream_id), ()))
            }
        });

        #[cfg(feature = "tracing")]
        let bi_stream_received = bi_stream_received
            .inspect(|bi_stream_received| {
                if let Ok(stream_id) = bi_stream_received {
                    debug!(?stream_id, "received incoming bidirectional stream");
                }
            })
            .in_current_span();

        bi_stream_received
    }

    /// Returns a [`Stream`] that yields whenever bytes are received.
    #[cfg_attr(feature = "tracing", instrument(level = "debug", skip(self)))]
    pub fn bytes_received(
        &self,
        stream_id: StreamId,
    ) -> impl Stream<Item = Result<Bytes, ReceiveBytesError>> + use<Sink> {
        let streams = self.streams.clone();

        let bytes_received = stream::unfold((), move |_| {
            let streams = streams.clone();
            let stream_id = stream_id.clone();
            async move {
                let streams = streams.pin_owned();

                let stream = match streams.get(&stream_id) {
                    Some(stream) => stream,
                    None => {
                        return Some((
                            Err(ReceiveBytesError {
                                kind: ReceiveBytesErrorKind::StreamNotFound,
                                inner: BoxError::from("could not find stream"),
                            }),
                            (),
                        ));
                    },
                };

                let mut rx = match stream {
                    crate::stream::Stream::OutgoingUni(_) => {
                        return Some((
                            Err(ReceiveBytesError {
                                kind: ReceiveBytesErrorKind::NonReadableStream,
                                inner: BoxError::from("stream is not a readable stream"),
                            }),
                            (),
                        ));
                    },
                    crate::stream::Stream::OutgoingBi(_, rx) => rx,
                    crate::stream::Stream::IncomingUni(rx) => rx,
                    crate::stream::Stream::IncomingBi(_, rx) => rx,
                }
                .write()
                .await;

                const CHUNK_SIZE: usize = 8 * 1024;

                let mut buf = BytesMut::with_capacity(CHUNK_SIZE);
                buf.resize(CHUNK_SIZE, 0u8);

                #[cfg(not(target_family = "wasm"))]
                match rx.read(&mut buf[..]).await {
                    #[cfg_attr(not(feature = "tracing"), allow(unused_variables))]
                    Ok(Some(bytes_read)) => {
                        #[cfg(feature = "tracing")]
                        trace!(bytes_read, "read from stream");

                        buf.truncate(bytes_read);
                    },
                    Ok(None) => return None,
                    Err(err) => {
                        return Some((
                            Err(ReceiveBytesError {
                                kind: ReceiveBytesErrorKind::Read,
                                inner: err.into(),
                            }),
                            (),
                        ));
                    },
                }
                #[cfg(all(target_family = "wasm", target_os = "unknown"))]
                match rx.read_buf(&mut buf).await {
                    #[cfg_attr(not(feature = "tracing"), allow(unused_variables))]
                    Ok(Some(bytes_read)) => {
                        #[cfg(feature = "tracing")]
                        trace!(bytes_read, "read from stream");

                        buf.truncate(bytes_read);
                    },
                    Ok(None) => return None,
                    Err(err) => {
                        return Some((
                            Err(ReceiveBytesError {
                                kind: ReceiveBytesErrorKind::Read,
                                inner: err.into(),
                            }),
                            (),
                        ));
                    },
                }

                Some((Ok(buf.into()), ()))
            }
        });

        #[cfg(feature = "tracing")]
        let bytes_received = bytes_received
            .inspect(|bytes_received| {
                if let Ok(bytes) = bytes_received {
                    debug!(?bytes, "received bytes from stream");
                }
            })
            .in_current_span();

        bytes_received
    }
}
