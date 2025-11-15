#![cfg(any(
    not(target_family = "wasm"),
    all(target_os = "wasi", target_env = "p2")
))]

use std::process::ExitCode;

use anyhow::Result;
use cyclers::driver::Driver as _;
use cyclers_terminal::{TerminalCommand, TerminalDriver, TerminalSource};
use futures_lite::stream;
#[cfg(not(target_family = "wasm"))]
use tokio::test;
#[cfg(all(target_os = "wasi", target_env = "p2"))]
use wstd::test;

#[test]
async fn it_returns_source_when_called_with_sink() {
    let sink = stream::pending::<TerminalCommand>();
    let (_source, _fut): (TerminalSource<_>, _) = TerminalDriver.call(sink);
}

#[test]
async fn it_exits_when_the_sink_stream_has_finished() -> Result<ExitCode> {
    let sink = stream::empty::<TerminalCommand>();
    let (_source, fut): (TerminalSource<_>, _) = TerminalDriver.call(sink);
    fut.await.map_err(anyhow::Error::from_boxed)
}
