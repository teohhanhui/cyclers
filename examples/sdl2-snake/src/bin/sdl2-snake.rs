use std::time::Duration;

use anyhow::Result;
use cyclers::BoxError;
use cyclers_sdl2::{Sdl2Command, Sdl2Driver, Sdl2Source, WindowPosition};
use futures_concurrency::stream::Chain as _;
use futures_lite::stream;
// use sdl2::event::Event;
// use sdl2::keyboard::Keycode;
// use sdl2::pixels::Color;
// use sdl2::sys::KeyCode;

const SIXTY_FPS: Duration = Duration::new(0, 1_000_000_000 / 60);

#[tokio::main]
async fn main() -> Result<()> {
    cyclers::run(
        |sdl2_source: Sdl2Source<_>| {
            let sdl2_sink = (stream::once(Ok::<_, BoxError>(Sdl2Command::CreateWindow {
                title: "Window".to_owned(),
                width: 800,
                height: 600,
                position: WindowPosition::Centered,
            })),)
                .chain();

            (sdl2_sink,)
        },
        (Sdl2Driver,),
    )
    .await
    .map_err(anyhow::Error::from_boxed)
}
