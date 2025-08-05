//! Run with:
//!
//! ```shell
//! cargo run --bin webrtc-chat
//! ```

use std::io;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use cyclers::{ArcError, BoxError};
use cyclers_terminal::{TerminalCommand, TerminalDriver, TerminalSource};
use cyclers_webrtc::{WebRtcCommand, WebRtcDriver, WebRtcSource};
use futures_concurrency::stream::{Chain as _, Merge as _, Zip as _};
use futures_lite::{StreamExt as _, stream};
use futures_rx::RxExt as _;
#[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
use tokio::main;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

#[main]
async fn main() -> Result<ExitCode> {
    init_tracing_subscriber();

    cyclers::run(
        |terminal_source: TerminalSource<_>, webrtc_source: WebRtcSource<_>| {
            // Configure the connection to the `matchbox_server`.
            let connect = stream::once(WebRtcCommand::Connect {
                room_url: format!(
                    "ws://{host}:{port}/{room_id}",
                    host = "127.0.0.1",
                    port = "3536",
                    room_id = "cyclers-webrtc-chat"
                ),
            });

            // Receive lines of input text that have been read from the terminal.
            let input = terminal_source
                .read_line()
                .map(|input| {
                    input
                        .context("failed to read line from terminal")
                        .map_err(|err| ArcError::from(BoxError::from(err)))
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

            // Broadcast each message to all connected peers.
            let connected_peers = webrtc_source.connected_peers();
            let send = (
                // Cache the latest list of connected peers.
                connected_peers
                    .map(Some)
                    .share_behavior(None)
                    .filter_map(|peers| (*peers).clone()),
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
                    .map_err(|err| ArcError::from(BoxError::from(err)));
                stream::iter([
                    Ok(TerminalCommand::Write("\n".to_owned())),
                    message
                        .map(|message| TerminalCommand::Write(format!("{peer_id}: {message}\n"))),
                    Ok(TerminalCommand::Write(">> ".to_owned())),
                    Ok(TerminalCommand::Flush),
                ])
            });

            let webrtc_sink = (
                // Connect to the `matchbox_server`.
                connect,
                // Send out any commands or messages.
                (slash_command, send).merge(),
            )
                .chain()
                .map(Ok::<_, ArcError>);

            // After reading a line from the terminal, we provide interactive feedback but
            // we do not print the line back out.
            let feedback = input.clone().map(|input| match &*input {
                Ok(_input) => stream::iter(vec![
                    Ok(TerminalCommand::Write("".to_owned())),
                    Ok(TerminalCommand::Flush),
                ]),
                Err(err) => stream::iter(vec![Err(Arc::clone(err))]),
            });

            let first_read = stream::iter([
                TerminalCommand::Write(">> ".to_owned()),
                TerminalCommand::Flush,
                TerminalCommand::ReadLine,
            ]);

            // Read the next line of user input from the terminal, as long as we have not
            // been asked to disconnect.
            let read = stream::stop_after_future(
                stream::repeat(stream::iter(vec![
                    TerminalCommand::Write(">> ".to_owned()),
                    TerminalCommand::Flush,
                    TerminalCommand::ReadLine,
                ])),
                {
                    let mut disconnect = disconnect.boxed();
                    async move {
                        disconnect.next().await;
                    }
                },
            );

            let bye = stream::once(stream::iter(vec![TerminalCommand::Write(
                "Bye!\n".to_owned(),
            )]));

            let terminal_sink = (
                // Interleave reads and feedback. Send a read command, and then wait to receive the
                // line being read and provide interactive feedback.
                (
                    first_read.map(Ok),
                    (feedback, read.or(bye))
                        .zip()
                        .flat_map(|(feedback, read)| (feedback, read.map(Ok)).chain()),
                )
                    .chain(),
                // Print out any messages immediately as they are received from peers.
                print_received,
            )
                .merge();

            (terminal_sink, webrtc_sink)
        },
        (TerminalDriver, WebRtcDriver),
    )
    .await
    .map_err(anyhow::Error::from_boxed)
}

fn init_tracing_subscriber() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                #[cfg(not(debug_assertions))]
                {
                    "info".into()
                }
                #[cfg(debug_assertions)]
                {
                    format!(
                        "{crate}=debug,cyclers=debug,cyclers_terminal=debug,cyclers_webrtc=debug",
                        crate = env!("CARGO_CRATE_NAME"),
                    )
                    .into()
                }
            }),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(io::stderr))
        .init();
}
