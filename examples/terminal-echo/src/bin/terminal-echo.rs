//! Run with:
//!
//! ```shell
//! cargo run --bin terminal-echo
//! ```
#![cfg_attr(
    all(
        target_family = "wasm",
        not(all(target_os = "wasi", target_env = "p2"))
    ),
    no_main
)]
#![cfg(any(
    not(target_family = "wasm"),
    all(target_os = "wasi", target_env = "p2")
))]

use std::io;
use std::process::ExitCode;

use anyhow::{Context as _, Result};
use cyclers_terminal::{TerminalCommand, TerminalDriver, TerminalSource};
use futures_concurrency::stream::{Chain as _, Zip as _};
use futures_lite::{StreamExt as _, stream};
#[cfg(not(target_family = "wasm"))]
use tokio::main;
#[cfg(target_family = "wasm")]
use tracing_subscriber::layer::Layer;
#[cfg(not(target_family = "wasm"))]
use tracing_subscriber::layer::Layer as _;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
#[cfg(all(target_os = "wasi", target_env = "p2"))]
use wstd::main;

#[main]
async fn main() -> Result<ExitCode> {
    init_tracing_subscriber();

    cyclers::run(
        |terminal_source: TerminalSource<_>| {
            // Each time we receive a line read from the terminal, we print it back out.
            let echo = terminal_source.lines().map(|input| {
                let input = input.context("failed to read line from terminal")?;
                Ok::<_, anyhow::Error>(TerminalCommand::Write(format!("{input}\n")))
            });

            let terminal_sink = (
                // Interleave reads and echoes. Send a read command, and then wait to receive the
                // line being read and print it back out.
                stream::once(Ok(TerminalCommand::ReadLine)),
                (echo, stream::repeat_with(|| Ok(TerminalCommand::ReadLine)))
                    .zip()
                    .flat_map(|(echo, read)| stream::iter([echo, read])),
            )
                .chain();

            (terminal_sink,)
        },
        (TerminalDriver,),
    )
    .await
    .map_err(anyhow::Error::from_boxed)
}

fn init_tracing_subscriber() {
    tracing_subscriber::registry()
        .with({
            #[cfg(not(target_family = "wasm"))]
            {
                Some(console_subscriber::spawn().boxed())
            }
            #[cfg(target_family = "wasm")]
            {
                None::<Box<dyn Layer<_> + Send + Sync + 'static>>
            }
        })
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(io::stderr)
                .with_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                        #[cfg(not(debug_assertions))]
                        {
                            "info".into()
                        }
                        #[cfg(debug_assertions)]
                        {
                            format!(
                                "{crate}=debug,cyclers=debug,cyclers_terminal=debug,info",
                                crate = env!("CARGO_CRATE_NAME"),
                            )
                            .into()
                        }
                    }),
                ),
        )
        .init();
}
