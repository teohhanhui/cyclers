use anyhow::{Context as _, Result};
use cyclers_terminal::{TerminalCommand, TerminalDriver, TerminalSource};
use futures_concurrency::stream::{Chain as _, Zip as _};
use futures_lite::{StreamExt as _, stream};
#[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
use tokio::main;
#[cfg(all(target_os = "wasi", target_env = "p2"))]
use wstd::main;

#[main]
async fn main() -> Result<()> {
    cyclers::run(
        |terminal_source: TerminalSource<_>| {
            // Each time we receive a line read from the terminal, we print it back out.
            let echo = terminal_source.read_line().map(|input| {
                let input = input.context("failed to read line from terminal")?;
                Ok::<_, anyhow::Error>(TerminalCommand::Write(format!("{input}\n")))
            });

            // Interleave reads and echoes. Send a read command, and then wait to
            // receive+print the line being read.
            let terminal_sink = (
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
