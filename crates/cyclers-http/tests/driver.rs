use anyhow::Result;
use cyclers::driver::Driver as _;
use cyclers_http::{HttpCommand, HttpDriver, HttpSource};
use futures_lite::stream;
#[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
use tokio::test;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use wasm_bindgen_test::wasm_bindgen_test as test;
#[cfg(all(target_os = "wasi", target_env = "p2"))]
use wstd::test;

#[test]
async fn it_returns_source_when_called_with_sink() {
    let sink = stream::pending::<HttpCommand>();
    let (_source, _fut): (HttpSource<_>, _) = HttpDriver.call(sink);
}

#[test]
async fn it_exits_when_the_sink_stream_has_finished() -> Result<()> {
    let sink = stream::empty::<HttpCommand>();
    let (_source, fut): (HttpSource<_>, _) = HttpDriver.call(sink);
    fut.await.map_err(anyhow::Error::from_boxed)
}
