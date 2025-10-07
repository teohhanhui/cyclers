//! Run with:
//!
//! ```shell
//! cargo run --bin webtrans-echo
//! ```

use std::io;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use bytes::{BufMut as _, BytesMut};
use cyclers::{ArcError, BoxError};
use cyclers_terminal::{TerminalCommand, TerminalDriver, TerminalSource};
use cyclers_webtrans::command::{
    CreateBiStreamCommand, EstablishSessionCommand, SendBytesCommand, TerminateSessionCommand,
};
use cyclers_webtrans::{WebTransportCommand, WebTransportDriver, WebTransportSource};
use futures_concurrency::stream::{Chain as _, Merge as _, Zip as _};
use futures_lite::{StreamExt as _, pin, stream};
use futures_rx::RxExt as _;
#[cfg(not(target_family = "wasm"))]
use tokio::main;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::debug;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

#[main]
async fn main() -> Result<ExitCode> {
    init_tracing_subscriber();

    cyclers::run(
        |terminal_source: TerminalSource<_>, webtrans_source: WebTransportSource<_>| {
            // Configure the connection to the WebTransport server.
            let establish_session =
                stream::once(WebTransportCommand::from(EstablishSessionCommand {
                    url: "https://wt-ams.akaleapi.net:6161/echo"
                        .try_into()
                        .expect("server URL should be valid"),
                }));

            // Wait for the connection to be established.
            //
            // Get the session ID when the connection has been established.
            let sessions = webtrans_source
                .session_ready()
                .map(|session| session.map_err(|err| ArcError::from(BoxError::from(err))))
                .share();
            let establish_session_errors = sessions.clone().filter_map(|session| match &*session {
                Ok(_) => None,
                Err(err) => Some(Err(Arc::clone(err))),
            });
            let session_ids = sessions
                .filter_map(|session| session.as_ref().ok().map(|(session_id, _url)| *session_id));

            // Read lines of text as input from the terminal.
            let lines = terminal_source
                .lines()
                .map(|line| {
                    line.context("failed to read line from terminal")
                        .map_err(|err| ArcError::from(BoxError::from(err)))
                })
                .share();
            let read_line_errors = lines.clone().filter_map(|line| match &*line {
                Ok(_) => None,
                Err(err) => Some(Err(Arc::clone(err))),
            });
            let lines = lines.filter_map(|line| match &*line {
                Ok(line) => Some(line.clone()),
                Err(_) => None,
            });

            // Filter down to the actual messages by removing slash (`/`) commands.
            let messages = lines.clone().filter_map(|line| {
                if line == "/quit" || line.is_empty() {
                    None
                } else {
                    Some(line.clone())
                }
            });

            // Translate the slash (`/`) commands into driver commands.
            let slash = (
                // Cache the latest session ID.
                session_ids.clone().switch_map(stream::repeat),
                lines.clone(),
            )
                .zip()
                .filter_map(|(session_id, line)| {
                    if line == "/quit" {
                        Some(WebTransportCommand::from(TerminateSessionCommand {
                            session_id,
                            code: Default::default(),
                            reason: Default::default(),
                        }))
                    } else {
                        None
                    }
                })
                .share();
            let terminate_session = slash
                .clone()
                .filter(|command| matches!(**command, WebTransportCommand::TerminateSession(..)));
            let slash = slash.map(|command| (*command).clone());

            const STREAM_IDS_BUFFER_LEN: usize = 1;
            let (stream_ids_tx, stream_ids_rx) = mpsc::channel(STREAM_IDS_BUFFER_LEN);
            let stream_ids = ReceiverStream::new(stream_ids_rx);

            // Send each message to the server.
            //
            // Create an outgoing bidirectional stream for each message.
            let send_message = (
                // Cache the latest session ID.
                session_ids
                    .switch_map(stream::repeat)
                    .inspect(|session_id| debug!(?session_id)),
                messages.inspect(|message_| debug!(?message_)),
            )
                .zip()
                .flat_map({
                    let webtrans_source = webtrans_source.clone();
                    move |(session_id, message)| {
                        (
                            stream::once(Ok(WebTransportCommand::from(CreateBiStreamCommand {
                                session_id,
                                send_order: Default::default(),
                            }))),
                            webtrans_source.bi_stream_created(session_id).then({
                                let stream_ids_tx = stream_ids_tx.clone();
                                move |stream_id| {
                                    let stream_ids_tx = stream_ids_tx.clone();
                                    let message = message.clone();
                                    async move {
                                        match stream_id {
                                            Ok(stream_id) => {
                                                let permit =
                                                    stream_ids_tx.reserve().await.map_err(
                                                        |err| ArcError::from(BoxError::from(err)),
                                                    )?;
                                                permit.send(stream_id.clone());
                                                Ok(WebTransportCommand::from(SendBytesCommand {
                                                    stream_id,
                                                    bytes: message.into(),
                                                    finished: true,
                                                }))
                                            },
                                            Err(err) => Err(ArcError::from(BoxError::from(err))),
                                        }
                                    }
                                }
                            }),
                        )
                            .chain()
                    }
                });

            // Print out messages we receive from the server to the terminal.
            let print_received = stream_ids
                .inspect(|stream_id| debug!(?stream_id))
                .flat_map(move |stream_id| {
                    let webtrans_source = webtrans_source.clone();
                    stream::once_future(async move {
                        let bytes_received = webtrans_source.bytes_received(stream_id);
                        pin!(bytes_received);
                        const INITIAL_CAPACITY: usize = 8 * 1024;
                        bytes_received
                            .try_fold(
                                BytesMut::with_capacity(INITIAL_CAPACITY),
                                move |mut buf, bytes| {
                                    buf.put(&bytes[..]);
                                    Ok(buf)
                                },
                            )
                            .await
                    })
                })
                .flat_map(|buf| match buf {
                    Ok(buf) => {
                        let message = String::from_utf8(buf.into())
                            .context("received message with invalid UTF-8")
                            .map_err(|err| ArcError::from(BoxError::from(err)));
                        stream::iter(vec![
                            Ok(TerminalCommand::Write("\n".to_owned())),
                            message.map(|message| {
                                TerminalCommand::Write(format!("server: {message}\n"))
                            }),
                            Ok(TerminalCommand::Write(">> ".to_owned())),
                            Ok(TerminalCommand::Flush),
                        ])
                    },
                    Err(err) => stream::iter(vec![Err(ArcError::from(BoxError::from(err)))]),
                });

            let webtrans_sink = (
                (
                    // Connect to the WebTransport server.
                    establish_session.map(Ok),
                    // Send out any commands or messages.
                    (slash.map(Ok), send_message).merge(),
                )
                    .chain(),
                establish_session_errors,
            )
                .merge();

            // After reading a line from the terminal, we provide interactive feedback but
            // we do not print the line back out.
            let feedback = lines.map(|_| {
                stream::iter(vec![
                    Ok(TerminalCommand::Write("".to_owned())),
                    Ok(TerminalCommand::Flush),
                ])
            });

            let first_read = stream::iter([
                TerminalCommand::Write(">> ".to_owned()),
                TerminalCommand::Flush,
                TerminalCommand::ReadLine,
            ]);

            // Read the next line of user input from the terminal, as long as we have not
            // been asked to terminate the session.
            let read = stream::stop_after_future(
                stream::repeat(stream::iter(vec![
                    TerminalCommand::Write(">> ".to_owned()),
                    TerminalCommand::Flush,
                    TerminalCommand::ReadLine,
                ])),
                {
                    let mut terminate_session = terminate_session.boxed();
                    async move {
                        terminate_session.next().await;
                    }
                },
            );

            let bye = stream::once(stream::iter(vec![TerminalCommand::Write(
                "Bye\n".to_owned(),
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
                print_received,
                read_line_errors,
            )
                .merge();

            (terminal_sink, webtrans_sink)
        },
        (TerminalDriver, WebTransportDriver),
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
                        "{crate}=debug,cyclers=debug,cyclers_terminal=debug,cyclers_webtrans=debug",
                        crate = env!("CARGO_CRATE_NAME"),
                    )
                    .into()
                }
            }),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(io::stderr))
        .init();
}
