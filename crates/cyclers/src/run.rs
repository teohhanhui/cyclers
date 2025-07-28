use std::error::Error;
use std::sync::Arc;

use futures_concurrency::future::TryJoin as _;
use futures_concurrency::stream::Merge as _;
use futures_lite::{Stream, StreamExt as _, pin};
use paste::paste;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::driver::{Driver, Source};

const SINK_PROXY_BUFFER_LEN: usize = 1;

/// Type alias for a type-erased error type.
pub type BoxError = Box<dyn Error + Send + Sync>;

/// Type alias for a type-erased error type that can be cloned.
pub type ArcError = Arc<dyn Error + Send + Sync>;

pub trait Main<Sources, Sinks> {
    fn call(self, sources: Sources) -> Sinks;
}

pub trait Drivers<Sinks> {
    type Sources: Sources;

    fn call(self, sinks: Sinks) -> (Self::Sources, impl Future<Output = Result<(), BoxError>>);
}

pub trait Sources {}

pub trait Sinks {
    type SinkSenders;
    type SinkReceivers;

    fn make_sink_proxies() -> (Self::SinkSenders, Self::SinkReceivers);

    fn replicate_many(
        self,
        sink_senders: Self::SinkSenders,
    ) -> impl Future<Output = Result<(), BoxError>> + MaybeSend;
}

#[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
pub trait MaybeSend: Send {}
#[cfg(any(target_family = "wasm", target_os = "wasi"))]
pub trait MaybeSend {}

macro_rules! impl_main {
    (
        $(($idx:tt, ($source:ident, $sink:ident))),+
    ) => {
        impl<F, $($source,)+ $($sink,)+> Main<($($source,)+), ($($sink,)+)> for F
        where
            F: FnOnce($($source),+) -> ($($sink,)+),
            $($source: Source,)+
            $($sink: Stream,)+
        {
            fn call(self, sources: ($($source,)+)) -> ($($sink,)+) {
                self($(sources.$idx),+)
            }
        }
    };
}

impl_main!((0, (Source1, Sink1)));
impl_main!((0, (Source1, Sink1)), (1, (Source2, Sink2)));
impl_main!(
    (0, (Source1, Sink1)),
    (1, (Source2, Sink2)),
    (2, (Source3, Sink3))
);
impl_main!(
    (0, (Source1, Sink1)),
    (1, (Source2, Sink2)),
    (2, (Source3, Sink3)),
    (3, (Source4, Sink4))
);
impl_main!(
    (0, (Source1, Sink1)),
    (1, (Source2, Sink2)),
    (2, (Source3, Sink3)),
    (3, (Source4, Sink4)),
    (4, (Source5, Sink5))
);
impl_main!(
    (0, (Source1, Sink1)),
    (1, (Source2, Sink2)),
    (2, (Source3, Sink3)),
    (3, (Source4, Sink4)),
    (4, (Source5, Sink5)),
    (5, (Source6, Sink6))
);
impl_main!(
    (0, (Source1, Sink1)),
    (1, (Source2, Sink2)),
    (2, (Source3, Sink3)),
    (3, (Source4, Sink4)),
    (4, (Source5, Sink5)),
    (5, (Source6, Sink6)),
    (6, (Source7, Sink7))
);
impl_main!(
    (0, (Source1, Sink1)),
    (1, (Source2, Sink2)),
    (2, (Source3, Sink3)),
    (3, (Source4, Sink4)),
    (4, (Source5, Sink5)),
    (5, (Source6, Sink6)),
    (6, (Source7, Sink7)),
    (7, (Source8, Sink8))
);
impl_main!(
    (0, (Source1, Sink1)),
    (1, (Source2, Sink2)),
    (2, (Source3, Sink3)),
    (3, (Source4, Sink4)),
    (4, (Source5, Sink5)),
    (5, (Source6, Sink6)),
    (6, (Source7, Sink7)),
    (7, (Source8, Sink8)),
    (8, (Source9, Sink9))
);
impl_main!(
    (0, (Source1, Sink1)),
    (1, (Source2, Sink2)),
    (2, (Source3, Sink3)),
    (3, (Source4, Sink4)),
    (4, (Source5, Sink5)),
    (5, (Source6, Sink6)),
    (6, (Source7, Sink7)),
    (7, (Source8, Sink8)),
    (8, (Source9, Sink9)),
    (9, (Source10, Sink10))
);
impl_main!(
    (0, (Source1, Sink1)),
    (1, (Source2, Sink2)),
    (2, (Source3, Sink3)),
    (3, (Source4, Sink4)),
    (4, (Source5, Sink5)),
    (5, (Source6, Sink6)),
    (6, (Source7, Sink7)),
    (7, (Source8, Sink8)),
    (8, (Source9, Sink9)),
    (9, (Source10, Sink10)),
    (10, (Source11, Sink11))
);
impl_main!(
    (0, (Source1, Sink1)),
    (1, (Source2, Sink2)),
    (2, (Source3, Sink3)),
    (3, (Source4, Sink4)),
    (4, (Source5, Sink5)),
    (5, (Source6, Sink6)),
    (6, (Source7, Sink7)),
    (7, (Source8, Sink8)),
    (8, (Source9, Sink9)),
    (9, (Source10, Sink10)),
    (10, (Source11, Sink11)),
    (11, (Source12, Sink12))
);

macro_rules! impl_drivers {
    (
        $(($idx:tt, ($sink:ident, $driver:ident))),+
    ) => {
        impl<$($sink,)+ $($driver,)+> Drivers<($($sink,)+)> for ($($driver,)+)
        where
            $($sink: Stream,)+
            $($driver: Driver<$sink>,)+
        {
            type Sources = ($($driver::Source,)+);

            fn call(
                self,
                sinks: ($($sink,)+),
            ) -> (Self::Sources, impl Future<Output = Result<(), BoxError>>) {
                paste! {
                    $(
                        let ([<source $idx>], [<fut $idx>]) = self.$idx.call(sinks.$idx);
                    )+

                    (($([<source $idx>],)+), async move {
                        ($([<fut $idx>],)+).try_join().await?;
                        Ok(())
                    })
                }
            }
        }
    };
}

impl_drivers!((0, (S1, D1)));
impl_drivers!((0, (S1, D1)), (1, (S2, D2)));
impl_drivers!((0, (S1, D1)), (1, (S2, D2)), (2, (S3, D3)));
impl_drivers!((0, (S1, D1)), (1, (S2, D2)), (2, (S3, D3)), (3, (S4, D4)));
impl_drivers!(
    (0, (S1, D1)),
    (1, (S2, D2)),
    (2, (S3, D3)),
    (3, (S4, D4)),
    (4, (S5, D5))
);
impl_drivers!(
    (0, (S1, D1)),
    (1, (S2, D2)),
    (2, (S3, D3)),
    (3, (S4, D4)),
    (4, (S5, D5)),
    (5, (S6, D6))
);
impl_drivers!(
    (0, (S1, D1)),
    (1, (S2, D2)),
    (2, (S3, D3)),
    (3, (S4, D4)),
    (4, (S5, D5)),
    (5, (S6, D6)),
    (6, (S7, D7))
);
impl_drivers!(
    (0, (S1, D1)),
    (1, (S2, D2)),
    (2, (S3, D3)),
    (3, (S4, D4)),
    (4, (S5, D5)),
    (5, (S6, D6)),
    (6, (S7, D7)),
    (7, (S8, D8))
);
impl_drivers!(
    (0, (S1, D1)),
    (1, (S2, D2)),
    (2, (S3, D3)),
    (3, (S4, D4)),
    (4, (S5, D5)),
    (5, (S6, D6)),
    (6, (S7, D7)),
    (7, (S8, D8)),
    (8, (S9, D9))
);
impl_drivers!(
    (0, (S1, D1)),
    (1, (S2, D2)),
    (2, (S3, D3)),
    (3, (S4, D4)),
    (4, (S5, D5)),
    (5, (S6, D6)),
    (6, (S7, D7)),
    (7, (S8, D8)),
    (8, (S9, D9)),
    (9, (S10, D10))
);
impl_drivers!(
    (0, (S1, D1)),
    (1, (S2, D2)),
    (2, (S3, D3)),
    (3, (S4, D4)),
    (4, (S5, D5)),
    (5, (S6, D6)),
    (6, (S7, D7)),
    (7, (S8, D8)),
    (8, (S9, D9)),
    (9, (S10, D10)),
    (10, (S11, D11))
);
impl_drivers!(
    (0, (S1, D1)),
    (1, (S2, D2)),
    (2, (S3, D3)),
    (3, (S4, D4)),
    (4, (S5, D5)),
    (5, (S6, D6)),
    (6, (S7, D7)),
    (7, (S8, D8)),
    (8, (S9, D9)),
    (9, (S10, D10)),
    (10, (S11, D11)),
    (11, (S12, D12))
);

macro_rules! impl_sources {
    (
        $($source:ident),+
    ) => {
        impl<$($source,)+> Sources for ($($source,)+) where $($source: Source,)+ {}
    };
}

impl_sources!(Source1);
impl_sources!(Source1, Source2);
impl_sources!(Source1, Source2, Source3);
impl_sources!(Source1, Source2, Source3, Source4);
impl_sources!(Source1, Source2, Source3, Source4, Source5);
impl_sources!(Source1, Source2, Source3, Source4, Source5, Source6);
impl_sources!(
    Source1, Source2, Source3, Source4, Source5, Source6, Source7
);
impl_sources!(
    Source1, Source2, Source3, Source4, Source5, Source6, Source7, Source8
);
impl_sources!(
    Source1, Source2, Source3, Source4, Source5, Source6, Source7, Source8, Source9
);
impl_sources!(
    Source1, Source2, Source3, Source4, Source5, Source6, Source7, Source8, Source9, Source10
);
impl_sources!(
    Source1, Source2, Source3, Source4, Source5, Source6, Source7, Source8, Source9, Source10,
    Source11
);
impl_sources!(
    Source1, Source2, Source3, Source4, Source5, Source6, Source7, Source8, Source9, Source10,
    Source11, Source12
);

macro_rules! impl_sinks {
    (
        $(($idx:tt, $sink:ident, $t:ident, $e:ident)),+
    ) => {
        impl<$($sink,)+ $($t,)+ $($e,)+> Sinks for ($($sink,)+)
        where
            $($sink: Stream<Item = Result<$t, $e>> + MaybeSend,)+
            $($sink::Item: MaybeSend,)+
            $($t: MaybeSend,)+
            $($e: Into<BoxError>,)+
        {
            type SinkReceivers = ($(ReceiverStream<$t>,)+);
            type SinkSenders = ($(mpsc::Sender<$t>,)+);

            fn make_sink_proxies() -> (Self::SinkSenders, Self::SinkReceivers) {
                paste! {
                    $(
                        let ([<tx $idx>], [<rx $idx>]) = mpsc::channel(SINK_PROXY_BUFFER_LEN);
                    )+

                    (($([<tx $idx>],)+), ($(ReceiverStream::new([<rx $idx>]),)+))
                }
            }

            #[allow(
                clippy::manual_async_fn,
                reason = "warning: use of `async fn` in public traits is discouraged as auto trait \
                          bounds cannot be specified"
            )]
            fn replicate_many(
                self,
                sink_senders: Self::SinkSenders,
            ) -> impl Future<Output = Result<(), BoxError>> + MaybeSend {
                async move {
                    let s = ($(self.$idx.then(|x| {
                        let tx = sink_senders.$idx.clone();
                        async move {
                            let x = x.map_err(Into::into)?;
                            let permit = tx.reserve().await.unwrap();
                            permit.send(x);
                            Ok::<_, BoxError>(())
                        }
                    }),)+)
                        .merge();
                    pin!(s);

                    while s.try_next().await?.is_some() {}
                    Ok(())
                }
            }
        }
    };
}

impl_sinks!((0, Sink1, T1, E1));
impl_sinks!((0, Sink1, T1, E1), (1, Sink2, T2, E2));
impl_sinks!((0, Sink1, T1, E1), (1, Sink2, T2, E2), (2, Sink3, T3, E3));
impl_sinks!(
    (0, Sink1, T1, E1),
    (1, Sink2, T2, E2),
    (2, Sink3, T3, E3),
    (3, Sink4, T4, E4)
);
impl_sinks!(
    (0, Sink1, T1, E1),
    (1, Sink2, T2, E2),
    (2, Sink3, T3, E3),
    (3, Sink4, T4, E4),
    (4, Sink5, T5, E5)
);
impl_sinks!(
    (0, Sink1, T1, E1),
    (1, Sink2, T2, E2),
    (2, Sink3, T3, E3),
    (3, Sink4, T4, E4),
    (4, Sink5, T5, E5),
    (5, Sink6, T6, E6)
);
impl_sinks!(
    (0, Sink1, T1, E1),
    (1, Sink2, T2, E2),
    (2, Sink3, T3, E3),
    (3, Sink4, T4, E4),
    (4, Sink5, T5, E5),
    (5, Sink6, T6, E6),
    (6, Sink7, T7, E7)
);
impl_sinks!(
    (0, Sink1, T1, E1),
    (1, Sink2, T2, E2),
    (2, Sink3, T3, E3),
    (3, Sink4, T4, E4),
    (4, Sink5, T5, E5),
    (5, Sink6, T6, E6),
    (6, Sink7, T7, E7),
    (7, Sink8, T8, E8)
);
impl_sinks!(
    (0, Sink1, T1, E1),
    (1, Sink2, T2, E2),
    (2, Sink3, T3, E3),
    (3, Sink4, T4, E4),
    (4, Sink5, T5, E5),
    (5, Sink6, T6, E6),
    (6, Sink7, T7, E7),
    (7, Sink8, T8, E8),
    (8, Sink9, T9, E9)
);
impl_sinks!(
    (0, Sink1, T1, E1),
    (1, Sink2, T2, E2),
    (2, Sink3, T3, E3),
    (3, Sink4, T4, E4),
    (4, Sink5, T5, E5),
    (5, Sink6, T6, E6),
    (6, Sink7, T7, E7),
    (7, Sink8, T8, E8),
    (8, Sink9, T9, E9),
    (9, Sink10, T10, E10)
);
impl_sinks!(
    (0, Sink1, T1, E1),
    (1, Sink2, T2, E2),
    (2, Sink3, T3, E3),
    (3, Sink4, T4, E4),
    (4, Sink5, T5, E5),
    (5, Sink6, T6, E6),
    (6, Sink7, T7, E7),
    (7, Sink8, T8, E8),
    (8, Sink9, T9, E9),
    (9, Sink10, T10, E10),
    (10, Sink11, T11, E11)
);
impl_sinks!(
    (0, Sink1, T1, E1),
    (1, Sink2, T2, E2),
    (2, Sink3, T3, E3),
    (3, Sink4, T4, E4),
    (4, Sink5, T5, E5),
    (5, Sink6, T6, E6),
    (6, Sink7, T7, E7),
    (7, Sink8, T8, E8),
    (8, Sink9, T9, E9),
    (9, Sink10, T10, E10),
    (10, Sink11, T11, E11),
    (11, Sink12, T12, E12)
);

#[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
impl<T: Send> MaybeSend for T {}
#[cfg(any(target_family = "wasm", target_os = "wasi"))]
impl<T> MaybeSend for T {}

pub fn setup<M, Drv, Snk>(
    main: M,
    drivers: Drv,
) -> (
    // Drv::Sources, Snk,
    impl AsyncFnOnce() -> Result<(), BoxError>,
)
where
    M: Main<Drv::Sources, Snk>,
    Drv: Drivers<Snk::SinkReceivers>,
    Snk: Sinks,
{
    let (sources, run) = setup_reusable(drivers);
    let sinks = main.call(sources);
    let run = async move || run(sinks).await;

    (/* sources, sinks, */ run,)
}

pub fn setup_reusable<Drv, Snk>(
    drivers: Drv,
) -> (Drv::Sources, impl AsyncFnOnce(Snk) -> Result<(), BoxError>)
where
    Drv: Drivers<Snk::SinkReceivers>,
    Snk: Sinks,
{
    let (sink_senders, sink_proxies) = Snk::make_sink_proxies();
    let (sources, drivers_fut) = drivers.call(sink_proxies);

    (sources, async |sinks: Snk| {
        (sinks.replicate_many(sink_senders), drivers_fut)
            .try_join()
            .await?;
        Ok(())
    })
}

pub async fn run<M, Drv, Snk>(main: M, drivers: Drv) -> Result<(), BoxError>
where
    M: Main<Drv::Sources, Snk>,
    Drv: Drivers<Snk::SinkReceivers>,
    Snk: Sinks,
{
    let (/* _sources, _sinks, */ run,) = setup(main, drivers);

    run().await
}
