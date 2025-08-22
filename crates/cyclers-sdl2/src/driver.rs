use cyclers::BoxError;
use cyclers::driver::{Driver, Source};
use futures_concurrency::future::TryJoin as _;
use futures_concurrency::stream::Zip as _;
use futures_lite::stream::Cycle;
use futures_lite::{Stream, StreamExt as _, pin, stream};
use futures_rx::stream_ext::share::Shared;
use futures_rx::{PublishSubject, RxExt as _};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::sys::KeyCode;
use sdl2::video::WindowPos;
use sdl2::{Sdl, VideoSubsystem};
#[cfg(feature = "tracing")]
use tracing::{debug, instrument};
#[cfg(feature = "tracing")]
use tracing_futures::Instrument as _;

pub struct Sdl2Driver;

pub struct Sdl2Source<Sink>
where
    Sink: Stream,
{
    sink: Shared<Sink, PublishSubject<Sink::Item>>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Sdl2Command {
    CreateWindow {
        title: String,
        width: u32,
        height: u32,
        position: WindowPosition,
    },
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub enum WindowPosition {
    #[default]
    Undefined,
    Centered,
    Positioned {
        x: i32,
        y: i32,
    },
}

impl<Sink> Driver<Sink> for Sdl2Driver
where
    Sink: Stream<Item = Sdl2Command>,
{
    type Source = Sdl2Source<Sink>;
    type Termination = ();

    fn call(
        self,
        sink: Sink,
    ) -> (
        Self::Source,
        impl Future<Output = Result<Self::Termination, BoxError>>,
    ) {
        let sink = sink.share();

        (
            Sdl2Source { sink: sink.clone() },
            self.run(sink),
        )
    }
}

impl Sdl2Driver {
    async fn run<Sink>(
        self,
        sink: Shared<Sink, PublishSubject<Sink::Item>>,
    ) -> Result<<Self as Driver<Sink>>::Termination, BoxError>
    where
        Sink: Stream<Item = Sdl2Command>,
    {
        let create_window = sink
            .filter(|command| matches!(**command, Sdl2Command::CreateWindow { .. }));

        let sdl_context = create_window
            .clone()
            .take(1)
            .map(|_| sdl2::init().map_err(BoxError::from))
            .cycle();
        let video_subsystem = sdl_context
            .clone()
            .take(1)
            .map(|sdl_context| {
                sdl_context.and_then(|sdl_context| sdl_context.video().map_err(BoxError::from))
            })
            .cycle();

        let window = stream::unfold(
            (video_subsystem, create_window).zip(),
            move |mut s| async move {
                let (video_subsystem, create_window) = s.next().await?;

                let video_subsystem = match video_subsystem {
                    Ok(video_subsystem) => video_subsystem,
                    Err(err) => {
                        return Some((Err(err), s));
                    },
                };

                #[allow(irrefutable_let_patterns)]
                let Sdl2Command::CreateWindow {
                    title,
                    width,
                    height,
                    position,
                } = &*create_window
                else {
                    unreachable!();
                };

                let mut window_builder = video_subsystem.window(title, *width, *height);
                let window_builder = match position {
                    WindowPosition::Undefined => &mut window_builder,
                    WindowPosition::Centered => window_builder.position_centered(),
                    WindowPosition::Positioned { x, y } => window_builder.position(*x, *y),
                };
                let window = match window_builder.build() {
                    Ok(window) => window,
                    Err(err) => {
                        return Some((Err(err.into()), s));
                    },
                };

                Some((Ok::<_, BoxError>(window), s))
            },
        );

        let run = (async move {
            pin!(window);

            // let mut event_pump = sdl_context.event_pump().unwrap();

            // 'running: loop {
            //     canvas.set_draw_color(Color::RGB(0, 64, 255));
            //     canvas.clear();

            //     for event in event_pump.poll_iter() {
            //         match event {
            //             sdl2::event::Event::Quit { .. }
            //             | Event::KeyDown {
            //                 keycode: Some(Keycode::Escape),
            //                 ..
            //             } => break 'running,
            //             _ => {}
            //         }
            //     }

            //     canvas.present();
            //     tokio::time::sleep(sixty_fps).await;
            // }

            while let Some(window) = window.try_next().await? {
                let mut canvas = window.into_canvas().build().unwrap();
                canvas.set_draw_color(Color::RGB(0, 255, 255));
                canvas.clear();
                canvas.present();
            }
            Ok(())
        },)
            .try_join();

        run.await.map(|_| ())
    }
}

impl<Sink> Source for Sdl2Source<Sink> where Sink: Stream {}

impl<Sink> Sdl2Source<Sink>
where
    Sink: Stream<Item = Sdl2Command>,
{
    // /// Returns a [`Stream`] that yields ???.
    // #[cfg_attr(feature = "tracing", instrument(level = "debug", skip(self)))]
    // pub fn init(&self) -> impl Stream<Item = Result<_, BoxError>> + use<Sink> {
    //     let create_window = self
    //         .sink
    //         .clone()
    //         .filter(|command| matches!(**command, Sdl2Command::CreateWindow { .. }));
    // }
}
