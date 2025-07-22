use std::error::Error;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use cyclers::driver::console::{ConsoleCommand, ConsoleDriver, ConsoleSource};
use cyclers::driver::webrtc::{WebRtcCommand, WebRtcDriver, WebRtcSource};
use futures_concurrency::stream::{Merge as _, Zip as _};
use futures_lite::{StreamExt as _, stream};
use futures_rx::{CombineLatest2, RxExt as _};

#[tokio::main]
async fn main() -> Result<()> {
    cyclers::run(
        |webrtc_source: WebRtcSource<_>, console_source: ConsoleSource<_>| {
            let connected_peers = webrtc_source.connected_peers();
            // Lines of input read from the console.
            let input = console_source
                .read()
                .map(|input| {
                    input.context("failed to read line").map_err(|err| {
                        Arc::<dyn Error + Send + Sync + 'static>::from(err.into_boxed_dyn_error())
                    })
                })
                .share();
            // Each time we receive a line read from the console, we send out an echo but we
            // do not print it back out.
            let pseudo_echo = input.clone().map(|input| match &*input {
                Ok(input) => Ok(ConsoleCommand::Print(input.clone())),
                Err(err) => Err(Arc::clone(err)),
            });
            // Filter down to the actual messages by removing slash (`/`) commands.
            let message = input.clone().filter_map(|input| match &*input {
                Ok(input) => {
                    if input == "/quit" || input.is_empty() {
                        None
                    } else {
                        Some(input.clone())
                    }
                },
                Err(_) => None,
            });
            // Translate the slash (`/`) commands into driver commands.
            let slash_command = input.clone().filter_map(|input| match &*input {
                Ok(input) => {
                    if input == "/quit" {
                        Some(WebRtcCommand::Disconnect)
                    } else {
                        None
                    }
                },
                Err(_) => None,
            });
            // Broadcast each message to all connected peers.
            let send = (
                // Cache the latest list of connected peers.
                CombineLatest2::new(connected_peers, stream::repeat(()))
                    .map(|(connected_peers, _)| connected_peers),
                message,
            )
                .zip()
                .flat_map(|(connected_peers, message)| {
                    stream::iter(connected_peers.into_iter().map(move |peer_id| {
                        WebRtcCommand::Send(
                            message.clone().into_bytes().into_boxed_slice(),
                            peer_id,
                        )
                    }))
                });
            // Print out messages we receive from other peers to the console.
            let print_received = webrtc_source.receive().flat_map(|(peer_id, packet)| {
                let message = String::from_utf8(packet.into())
                    .context("received message with invalid UTF-8")
                    .map_err(|err| {
                        Arc::<dyn Error + Send + Sync + 'static>::from(err.into_boxed_dyn_error())
                    });
                stream::iter([
                    message.map(|message| ConsoleCommand::Print(format!("{peer_id}: {message}"))),
                    Ok(ConsoleCommand::Print("".to_owned())),
                    Ok(ConsoleCommand::Print(">> ".to_owned())),
                ])
            });
            // Send out any slash commands or messages.
            //
            // But before anything else, connect to the `matchbox_server`.
            let webrtc_sink = (slash_command, send)
                .merge()
                .map(Ok::<_, Arc<dyn Error + Send + Sync + 'static>>)
                .start_with([Ok(WebRtcCommand::Connect {
                    room_url: format!(
                        "ws://{host}:{port}/{room_id}",
                        host = "127.0.0.1",
                        port = "3536",
                        room_id = "cyclers-webrtc-chat"
                    ),
                })]);
            let console_sink = (
                // Interleave reads and pseudo-echoes. Send a read command, and then wait to
                // receive+pseudo-print the line being read.
                (
                    pseudo_echo,
                    stream::repeat_with(|| Ok(ConsoleCommand::Read)),
                )
                    .zip()
                    .flat_map(|(echo, read)| {
                        stream::iter([
                            echo.map(|_| ConsoleCommand::Print("".to_owned())),
                            Ok(ConsoleCommand::Print(">> ".to_owned())),
                            read,
                        ])
                    })
                    .start_with([
                        Ok(ConsoleCommand::Print(">> ".to_owned())),
                        Ok(ConsoleCommand::Read),
                    ]),
                // Don't worry. Received messages are actually being printed.
                print_received,
            )
                .merge();

            (webrtc_sink, console_sink)
        },
        (WebRtcDriver, ConsoleDriver),
    )
    .await
    .map_err(anyhow::Error::from_boxed)
}
