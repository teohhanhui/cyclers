use std::sync::Arc;

use anyhow::{Context as _, Result};
use cyclers::ArcError;
use cyclers::driver::terminal::{TerminalCommand, TerminalDriver, TerminalSource};
use cyclers::driver::webrtc::{WebRtcCommand, WebRtcDriver, WebRtcSource};
use futures_concurrency::stream::{Chain as _, Merge as _, Zip as _};
use futures_lite::{StreamExt as _, stream};
use futures_rx::{CombineLatest2, RxExt as _};

#[tokio::main]
async fn main() -> Result<()> {
    cyclers::run(
        |webrtc_source: WebRtcSource<_>, terminal_source: TerminalSource<_>| {
            let connect = stream::once(WebRtcCommand::Connect {
                room_url: format!(
                    "ws://{host}:{port}/{room_id}",
                    host = "127.0.0.1",
                    port = "3536",
                    room_id = "cyclers-webrtc-chat"
                ),
            });

            // Read lines of input from the terminal.
            let input = terminal_source
                .read_line()
                .map(|input| {
                    input
                        .context("failed to read line from terminal")
                        .map_err(|err| ArcError::from(err.into_boxed_dyn_error()))
                })
                .share();

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
            let slash_command = input
                .clone()
                .filter_map(|input| match &*input {
                    Ok(input) => {
                        if input == "/quit" {
                            Some(WebRtcCommand::Disconnect)
                        } else {
                            None
                        }
                    },
                    Err(_) => None,
                })
                .share();
            let disconnect = slash_command
                .clone()
                .filter(|command| matches!(**command, WebRtcCommand::Disconnect));
            let slash_command = slash_command.map(|command| (*command).clone());

            // Each time we receive a line read from the terminal, we send out an echo but
            // we do not print it back out.
            let pseudo_echo = input.clone().map(|input| match &*input {
                Ok(_input) => stream::iter(vec![
                    Ok(TerminalCommand::Write("".to_owned())),
                    Ok(TerminalCommand::Flush),
                ]),
                Err(err) => stream::iter(vec![Err(Arc::clone(err))]),
            });

            // Stop reading input after we have sent the disconnect command.
            let read_until_disconnect =
                stream::stop_after_future(stream::repeat(TerminalCommand::ReadLine), async move {
                    let mut disconnect = disconnect.boxed();
                    disconnect.next().await;
                });

            // Broadcast each message to all connected peers.
            let connected_peers = webrtc_source.connected_peers();
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

            // Print out messages we receive from other peers to the terminal.
            let print_received = webrtc_source.receive().flat_map(|(peer_id, packet)| {
                let message = String::from_utf8(packet.into())
                    .context("received message with invalid UTF-8")
                    .map_err(|err| ArcError::from(err.into_boxed_dyn_error()));
                stream::iter([
                    Ok(TerminalCommand::Write("\n".to_owned())),
                    message
                        .map(|message| TerminalCommand::Write(format!("{peer_id}: {message}\n"))),
                    Ok(TerminalCommand::Write(">> ".to_owned())),
                    Ok(TerminalCommand::Flush),
                ])
            });

            // First, connect to the `matchbox_server`.
            //
            // Then continue to send out any commands or messages.
            let webrtc_sink = (connect, (slash_command, send).merge())
                .chain()
                .map(Ok::<_, ArcError>);

            let terminal_sink = (
                // Interleave reads and pseudo-echoes. Send a read command, and then wait to
                // receive+pseudo-print the line being read.
                (
                    stream::iter([
                        TerminalCommand::Write(">> ".to_owned()),
                        TerminalCommand::Flush,
                        TerminalCommand::ReadLine,
                    ])
                    .map(Ok),
                    (
                        pseudo_echo,
                        read_until_disconnect
                            .map(|read| {
                                stream::iter(vec![
                                    TerminalCommand::Write(">> ".to_owned()),
                                    TerminalCommand::Flush,
                                    read,
                                ])
                            })
                            .or(stream::once(stream::iter(vec![TerminalCommand::Write(
                                "Bye!\n".to_owned(),
                            )]))),
                    )
                        .zip()
                        .flat_map(|(echo, read)| (echo, read.map(Ok)).chain()),
                )
                    .chain(),
                // Don't worry. Received messages are actually being printed.
                print_received,
            )
                .merge();

            (webrtc_sink, terminal_sink)
        },
        (WebRtcDriver, TerminalDriver),
    )
    .await
    .map_err(anyhow::Error::from_boxed)
}
