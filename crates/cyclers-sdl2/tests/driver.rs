use anyhow::Result;
use cyclers::driver::Driver as _;
use cyclers_sdl2::{Sdl2Command, Sdl2Driver, Sdl2Source};
use futures_lite::stream;
#[cfg(not(target_family = "wasm"))]
use tokio::test;

#[test]
async fn it_returns_source_when_called_with_sink() {
    let sink = stream::pending::<Sdl2Command>();
    let (_source, _fut): (Sdl2Source<_>, _) = Sdl2Driver.call(sink);
}

#[test]
async fn it_exits_when_the_sink_stream_has_finished() -> Result<()> {
    let sink = stream::empty::<Sdl2Command>();
    let (_source, fut): (Sdl2Source<_>, _) = Sdl2Driver.call(sink);
    fut.await.map_err(anyhow::Error::from_boxed)
}
