use anyhow::{Context as _, Result};
use cyclers::driver::console::{ConsoleCommand, ConsoleDriver, ConsoleSource};
use futures_concurrency::stream::{Chain as _, Zip as _};
use futures_lite::{StreamExt as _, stream};

#[tokio::main]
async fn main() -> Result<()> {
    cyclers::run(
        |console_source: ConsoleSource<_>| {
            // Each time we receive a line read from the console, we print it back out.
            let echo = console_source.read().map(|input| {
                let input = input.context("failed to read line from console")?;
                Ok::<_, anyhow::Error>(ConsoleCommand::Print(input))
            });

            // Interleave reads and echoes. Send a read command, and then wait to
            // receive+print the line being read.
            let console_sink = (
                stream::once(Ok(ConsoleCommand::Read)),
                (echo, stream::repeat_with(|| Ok(ConsoleCommand::Read)))
                    .zip()
                    .flat_map(|(echo, read)| stream::iter([echo, read])),
            )
                .chain();

            (console_sink,)
        },
        (ConsoleDriver,),
    )
    .await
    .map_err(anyhow::Error::from_boxed)
}
