use cyclers::driver::console::{ConsoleCommand, ConsoleDriver, ConsoleSource};
use futures_concurrency::stream::Zip as _;
use futures_lite::{StreamExt as _, stream};
use futures_rx::RxExt as _;

#[tokio::main]
async fn main() {
    cyclers::run(
        |console_source: ConsoleSource<_>| {
            // Each time we receive a line read from the console, we print it back out.
            let echo = console_source
                .read()
                .map(|input| ConsoleCommand::Print(input.unwrap()));
            // Interleave reads and echoes. Send a read command, and then wait to
            // receive+print the line being read.
            let console_sink = (echo, stream::repeat(ConsoleCommand::Read))
                .zip()
                .flat_map(|(echo, read)| stream::iter([echo, read]))
                .start_with([ConsoleCommand::Read]);

            (console_sink,)
        },
        (ConsoleDriver,),
    )
    .await
}
