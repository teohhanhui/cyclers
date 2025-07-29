use std::collections::HashMap;

use cyclers::BoxError;
use cyclers::driver::{Driver, Source};
use futures_concurrency::future::{Race as _, TryJoin as _};
use futures_concurrency::stream::Zip as _;
use futures_lite::stream::Cycle;
use futures_lite::{Stream, StreamExt as _, future, pin, stream};
use futures_rx::stream_ext::share::Shared;
use futures_rx::{PublishSubject, ReplaySubject, RxExt as _};
use matchbox_socket::WebRtcSocket;
pub use matchbox_socket::{Packet, PeerId, PeerState};
use tokio::sync::{Mutex, oneshot};
use tokio_util::sync::CancellationToken;

/// Type alias for an infinite [`Stream`] that yields the same [`WebRtcSocket`]
/// item repeatedly.
type Socket = Cycle<Shared<stream::Boxed<Mutex<WebRtcSocket>>, ReplaySubject<Mutex<WebRtcSocket>>>>;

/// Type alias for an infinite [`Stream`] that yields the same channel receiver
/// item repeatedly.
type ChannelReceiver = Cycle<
    Shared<
        stream::Boxed<Mutex<futures_channel::mpsc::UnboundedReceiver<(PeerId, Packet)>>>,
        ReplaySubject<Mutex<futures_channel::mpsc::UnboundedReceiver<(PeerId, Packet)>>>,
    >,
>;

pub struct WebRtcDriver;

pub struct WebRtcSource<Sink>
where
    Sink: Stream,
{
    // TODO: Will we have request-response type of commands?
    _sink: Shared<Sink, PublishSubject<Sink::Item>>,
    socket: Socket,
    channel_receiver: ChannelReceiver,
    close_token: CancellationToken,
}

#[derive(Clone, Eq, PartialEq, Debug)]
#[non_exhaustive]
pub enum WebRtcCommand {
    Connect { room_url: String },
    Disconnect,
    Send(Packet, PeerId),
}

impl<Sink> Driver<Sink> for WebRtcDriver
where
    Sink: Stream<Item = WebRtcCommand>,
{
    type Input = WebRtcCommand;
    type Source = WebRtcSource<Sink>;

    fn call(self, sink: Sink) -> (Self::Source, impl Future<Output = Result<(), BoxError>>) {
        let sink = sink.share();
        let connect = sink
            .clone()
            .filter(|command| matches!(**command, WebRtcCommand::Connect { .. }));
        let disconnect = sink
            .clone()
            .filter(|command| matches!(**command, WebRtcCommand::Disconnect));
        let send = sink
            .clone()
            .filter(|command| matches!(**command, WebRtcCommand::Send(..)));

        // Set up oneshot channels for getting things back out from the
        // `WebRtcCommand::Connect` handler.
        let (socket_tx, socket_rx) = oneshot::channel();
        let (channel_sender_tx, channel_sender_rx) = oneshot::channel();
        let (channel_receiver_tx, channel_receiver_rx) = oneshot::channel();
        // Use `.share_replay().cycle()` to cache the socket from the oneshot channel.
        let socket = stream::once_future(socket_rx)
            .map(|socket| {
                let socket = socket.expect("`socket_tx` dropped without sending");
                Mutex::new(socket)
            })
            .boxed()
            .share_replay()
            .cycle();
        // Use `.share_replay().cycle()` to cache the channel sender from the oneshot
        // channel.
        let channel_sender = stream::once_future(channel_sender_rx)
            .map(|channel_sender| {
                channel_sender.expect("`channel_sender_tx` dropped without sending")
            })
            .share_replay()
            .cycle();
        // Use `.share_replay().cycle()` to cache the channel receiver from the oneshot
        // channel.
        let channel_receiver = stream::once_future(channel_receiver_rx)
            .map(|channel_receiver| {
                let channel_receiver =
                    channel_receiver.expect("`channel_receiver_tx` dropped without sending");
                Mutex::new(channel_receiver)
            })
            .boxed()
            .share_replay()
            .cycle();
        let message_loop_fut = async move {
            let mut connect = connect;
            // TODO: Handle multiple connections?
            if let Some(connect) = connect.next().await {
                let WebRtcCommand::Connect { room_url } = &*connect else {
                    unreachable!();
                };
                let (mut socket, message_loop_fut) = WebRtcSocket::new_reliable(room_url);
                // Split up the `matchbox_socket::WebRtcChannel` into sender and receiver.
                //
                // Note: They are not connected to each other in the first place. The "channel"
                // in the name refers to WebRTC data channel, not channels in the Rust sense...
                //
                // We need to be able to poll both at the same time without causing a deadlock
                // accessing the socket as a shared resource.
                //
                // TODO: Handle multiple channels per connection?
                let (channel_sender, channel_receiver) = socket.take_channel(0).unwrap().split();
                // Send things back out through oneshot channels so that they can be used
                // elsewhere in this driver.
                socket_tx.send(socket).expect("`socket_rx` dropped");
                channel_sender_tx
                    .send(channel_sender)
                    .expect("`channel_sender_rx` dropped");
                channel_receiver_tx
                    .send(channel_receiver)
                    .expect("`channel_receiver_rx` dropped");
                // This message loop future from `matchbox_socket` will need to be polled in the
                // driver future.
                return message_loop_fut.await;
            }
            Ok(())
        };
        let close_token = CancellationToken::new();

        (
            WebRtcSource {
                _sink: sink,
                socket,
                channel_receiver: channel_receiver.clone(),
                close_token: close_token.child_token(),
            },
            async move {
                (
                    {
                        let channel_sender = channel_sender.clone();
                        let close_token = close_token.child_token();
                        async move {
                            let s = stream::unfold((channel_sender, send).zip(), |mut s| {
                                let close_token = &close_token;
                                async move {
                                    let (channel_sender, send) = (s.next(), async move {
                                        close_token.cancelled().await;
                                        None
                                    })
                                        .race()
                                        .await?;
                                    let WebRtcCommand::Send(packet, peer_id) = &*send else {
                                        unreachable!();
                                    };
                                    Some((
                                        channel_sender
                                            .unbounded_send((*peer_id, packet.clone()))
                                            .map_err(BoxError::from),
                                        s,
                                    ))
                                }
                            });
                            pin!(s);

                            while s.try_next().await?.is_some() {}
                            // HACK: Prevent the driver future from being dropped.
                            future::pending::<()>().await;
                            Ok(())
                        }
                    },
                    async move {
                        let s = (channel_sender, channel_receiver, disconnect).zip().then(
                            |(channel_sender, channel_receiver, disconnect)| {
                                let WebRtcCommand::Disconnect = &*disconnect else {
                                    unreachable!();
                                };
                                let close_token = &close_token;
                                async move {
                                    // Signal the `WebRtcCommand::Send` handler and the
                                    // `WebRtcSource::receive` stream to stop.
                                    close_token.cancel();
                                    // Close channel sender and receiver. `WebRtcSource::receive`
                                    // stream should have stopped / should stop soon, so there
                                    // should be no deadlock on the mutex here.
                                    channel_sender.close_channel();
                                    let mut channel_receiver = channel_receiver.lock().await;
                                    channel_receiver.close();
                                }
                            },
                        );
                        pin!(s);

                        s.next().await;
                        // HACK: Prevent the driver future from being dropped.
                        future::pending::<()>().await;
                        Ok(())
                    },
                    async move {
                        // Poll the message loop future from `matchbox_socket`.
                        message_loop_fut.await?;
                        // HACK: Prevent the driver future from being dropped.
                        future::pending::<()>().await;
                        Ok::<_, BoxError>(())
                    },
                )
                    .try_join()
                    .await
                    .map(|_| ())
            },
        )
    }
}

impl<Sink> Source for WebRtcSource<Sink> where Sink: Stream {}

impl<Sink> WebRtcSource<Sink>
where
    Sink: Stream<Item = WebRtcCommand>,
{
    // pub fn id(&self) -> impl Stream<Item = PeerId> + use<Sink>
    // {
    //     todo!()
    // }

    /// Returns a [`Stream`] that yields peer connected/disconnected state
    /// changes.
    pub fn peer_changes(&self) -> impl Stream<Item = (PeerId, PeerState)> + use<Sink> {
        let socket = self.socket.clone();

        stream::unfold((socket,), {
            move |(mut socket,)| async move {
                let sock = &*socket.next().await.unwrap();
                let mut sock = sock.lock().await;
                // `WebRtcSocket` itself is a `Stream`, which yields peer changes.
                let (peer_id, peer_state) = sock.next().await?;

                Some(((peer_id, peer_state), (socket,)))
            }
        })
    }

    /// Returns a [`Stream`] that yields the latest list of all connected peers.
    pub fn connected_peers(&self) -> impl Stream<Item = Vec<PeerId>> + use<Sink> {
        let peer_changes = self.peer_changes();
        let peer_changes = Box::pin(peer_changes);

        stream::unfold(
            (None, peer_changes),
            move |(mut peers_storage, mut peer_changes): (
                Option<HashMap<PeerId, PeerState>>,
                _,
            )| async move {
                let (peer_id, peer_state) = peer_changes.next().await?;
                if let Some(peers) = &mut peers_storage {
                    peers.insert(peer_id, peer_state);

                    Some((
                        peers.keys().copied().collect(),
                        (peers_storage, peer_changes),
                    ))
                } else {
                    peers_storage = Some(HashMap::from([(peer_id, peer_state)]));

                    Some((vec![peer_id], (peers_storage, peer_changes)))
                }
            },
        )
    }

    /// Returns a [`Stream`] that yields messages received from other peers.
    pub fn receive(&self) -> impl Stream<Item = (PeerId, Packet)> + use<Sink> {
        let channel_receiver = self.channel_receiver.clone();
        let close_token = self.close_token.clone();

        stream::unfold(
            (channel_receiver, close_token),
            move |(mut channel_receiver, close_token)| async move {
                let channel_rx = &*channel_receiver.next().await.unwrap();
                let mut channel_rx = channel_rx.lock().await;
                let (peer_id, packet) = (channel_rx.next(), {
                    let close_token = &close_token;
                    async move {
                        close_token.cancelled().await;
                        None
                    }
                })
                    .race()
                    .await?;

                Some(((peer_id, packet), (channel_receiver, close_token)))
            },
        )
    }
}
