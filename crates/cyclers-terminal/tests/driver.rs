use cyclers::driver::Driver as _;
use cyclers_terminal::{TerminalCommand, TerminalDriver, TerminalSource};
use futures_lite::stream;
#[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
use tokio::test;
#[cfg(all(target_os = "wasi", target_env = "p2"))]
use wstd::test;

#[test]
async fn it_returns_source_when_called_with_sink() {
    let sink = stream::pending::<TerminalCommand>();
    let (_source, _fut): (TerminalSource<_>, _) = TerminalDriver.call(sink);
}
