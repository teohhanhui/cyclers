use cyclers::driver::Driver as _;
use cyclers_terminal::{TerminalCommand, TerminalDriver, TerminalSource};
use futures_lite::stream;

#[tokio::test]
async fn it_returns_source_when_called_with_sink() {
    let sink = stream::pending::<TerminalCommand>();
    let (_source, _fut): (TerminalSource<_>, _) = TerminalDriver.call(sink);
}
