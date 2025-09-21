use anyhow::Result;
use cyclers::driver::Driver as _;
use cyclers_webtrans::{WebTransportCommand, WebTransportDriver, WebTransportSource};
use futures_lite::stream;
#[cfg(not(target_family = "wasm"))]
use tokio::test;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use wasm_bindgen_test::wasm_bindgen_test as test;

#[test]
async fn it_returns_source_when_called_with_sink() {
    let sink = stream::pending::<WebTransportCommand>();
    let (_source, _fut): (WebTransportSource<_>, _) = WebTransportDriver.call(sink);
}

#[test]
async fn it_exits_when_the_sink_stream_has_finished() -> Result<()> {
    let sink = stream::empty::<WebTransportCommand>();
    let (_source, fut): (WebTransportSource<_>, _) = WebTransportDriver.call(sink);
    fut.await.map_err(anyhow::Error::from_boxed)
}
