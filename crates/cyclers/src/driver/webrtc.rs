use std::collections::HashMap;

use futures_concurrency::future::Join as _;
use futures_concurrency::stream::Zip as _;
use futures_core::Stream;
use futures_lite::stream::Cycle;
use futures_lite::{StreamExt as _, future, stream};
use futures_rx::stream_ext::share::Shared;
use futures_rx::{CombineLatest2, PublishSubject, ReplaySubject, RxExt as _};
use matchbox_socket::WebRtcSocket;
pub use matchbox_socket::{Packet, PeerId, PeerState};
use tokio::sync::{Mutex, oneshot};

use super::{Driver, Source};

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
}

#[derive(Clone, Eq, PartialEq, Debug)]
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

    fn call(self, sink: Sink) -> (Self::Source, impl Future<Output = ()>) {
        let sink = sink.share();
        let mut connect = sink
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
            if let Some(command) = connect.next().await {
                match &*command {
                    WebRtcCommand::Connect { room_url } => {
                        let (mut socket, message_loop_fut) = WebRtcSocket::new_reliable(room_url);
                        // Split up the `matchbox_socket::WebRtcChannel` into sender and receiver.
                        //
                        // Note: They are not connected to each other in the first place. The
                        // "channel" in the name refers to WebRTC data channel, not channels in the
                        // Rust sense...
                        //
                        // We need to be able to poll both at the same time without causing a
                        // deadlock accessing the socket as a shared resource.
                        //
                        // TODO: Handle multiple channels per connection?
                        let (channel_sender, channel_receiver) =
                            socket.take_channel(0).unwrap().split();
                        // Send things back out through oneshot channels so that they can be used
                        // elsewhere in this driver.
                        socket_tx.send(socket).expect("`socket_rx` dropped");
                        channel_sender_tx
                            .send(channel_sender)
                            .expect("`channel_sender_rx` dropped");
                        channel_receiver_tx
                            .send(channel_receiver)
                            .expect("`channel_receiver_rx` dropped");
                        // This message loop future from `matchbox_socket` will need to be polled in
                        // the driver future.
                        return message_loop_fut.await;
                    },
                    _ => unreachable!(),
                }
            }
            // TODO: Handle multiple connections?
            future::pending().await
        };

        (
            WebRtcSource {
                _sink: sink,
                socket: socket.clone(),
                channel_receiver,
            },
            async move {
                (
                    (
                        // (Ab)use `CombineLatest2` to cache the channel sender from the oneshot
                        // channel.
                        CombineLatest2::new(
                            stream::once_future(channel_sender_rx).map(|socket| {
                                socket.expect("`channel_sender_tx` dropped without sending")
                            }),
                            stream::repeat(()),
                        )
                        .map(|(channel_sender, _)| channel_sender),
                        send,
                    )
                        .zip()
                        .then(|(channel_sender, command)| match &*command {
                            WebRtcCommand::Send(packet, peer_id) => {
                                let packet = packet.clone();
                                let peer_id = *peer_id;
                                async move {
                                    if let Err(err) =
                                        channel_sender.unbounded_send((peer_id, packet))
                                    {
                                        todo!("unhandled error: {err}");
                                    }
                                }
                            },
                            _ => unreachable!(),
                        })
                        .last(),
                    disconnect
                        .then(|command| match &*command {
                            WebRtcCommand::Disconnect => {
                                let mut socket = socket.clone();
                                async move {
                                    let socket = &*socket.next().await.unwrap();
                                    let mut socket = socket.lock().await;
                                    socket.close();
                                }
                            },
                            _ => unreachable!(),
                        })
                        .last(),
                    async move {
                        // Poll the message loop future from `matchbox_socket`.
                        if let Err(err) = message_loop_fut.await {
                            todo!("unhandled error: {err}");
                        }
                    },
                )
                    .join()
                    .await;
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
        stream::unfold((), {
            let socket = self.socket.clone();
            move |_| {
                let mut socket = socket.clone();
                async move {
                    let socket = &*socket.next().await.unwrap();
                    let mut socket = socket.lock().await;
                    // `WebRtcSocket` itself is a `Stream`, which yields peer changes.
                    if let Some((peer_id, peer_state)) = socket.next().await {
                        Some(((peer_id, peer_state), ()))
                    } else {
                        None
                    }
                }
            }
        })
    }

    /// Returns a [`Stream`] that yields the latest list of all connected peers.
    pub fn connected_peers(&self) -> impl Stream<Item = Vec<PeerId>> + use<Sink> {
        let peer_changes = self.peer_changes().share();
        stream::unfold(
            None,
            move |mut peers_storage: Option<HashMap<PeerId, PeerState>>| {
                let mut peer_changes = peer_changes.clone();
                async move {
                    if let Some(peer) = peer_changes.next().await {
                        let (peer_id, peer_state) = &*peer;
                        if let Some(peers) = &mut peers_storage {
                            peers.insert(*peer_id, *peer_state);
                            Some((peers.keys().copied().collect(), peers_storage))
                        } else {
                            peers_storage = Some(HashMap::from([(*peer_id, *peer_state)]));
                            Some((vec![*peer_id], peers_storage))
                        }
                    } else {
                        None
                    }
                }
            },
        )
    }

    /// Returns a [`Stream`] that yields messages received from other peers.
    pub fn receive(&self) -> impl Stream<Item = (PeerId, Packet)> + use<Sink> {
        stream::unfold((), {
            let channel_receiver = self.channel_receiver.clone();
            move |_| {
                let mut channel_receiver = channel_receiver.clone();
                async move {
                    let channel_receiver = &*channel_receiver.next().await.unwrap();
                    let mut channel_receiver = channel_receiver.lock().await;
                    if let Some((peer_id, packet)) = channel_receiver.next().await {
                        Some(((peer_id, packet), ()))
                    } else {
                        None
                    }
                }
            }
        })
    }
}
