use futures_concurrency::future::Join as _;
use futures_concurrency::stream::Merge as _;
use futures_core::Stream;
use futures_lite::StreamExt as _;
use paste::paste;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::driver::{Driver, Source};

const SINK_PROXY_BUFFER_LEN: usize = 1;

pub trait Main<Sources, Sinks> {
    fn call(self, sources: Sources) -> Sinks;
}

pub trait Drivers<Inputs> {
    type SinkProxies;
    type Sources: Sources;

    fn call(self, sink_proxies: Self::SinkProxies) -> (Self::Sources, impl Future<Output = ()>);
}

pub trait Sources {}

pub trait Sinks {
    type SinkSenders;
    type SinkReceivers;

    fn make_sink_proxies() -> (Self::SinkSenders, Self::SinkReceivers);

    fn replicate_many(self, sink_senders: Self::SinkSenders) -> impl Future<Output = ()>;
}

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
        $(($idx:tt, ($t:ident, $driver:ident))),+
    ) => {
        impl<$($t,)+ $($driver,)+> Drivers<($($t,)+)> for ($($driver,)+)
        where
            $($driver: Driver<ReceiverStream<$t>>,)+
        {
            type SinkProxies = ($(ReceiverStream<$t>,)+);
            type Sources = ($($driver::Source,)+);

            fn call(
                self,
                sink_proxies: Self::SinkProxies,
            ) -> (Self::Sources, impl Future<Output = ()>) {
                paste! {
                    $(
                        let ([<source $idx>], [<fut $idx>]) = self.$idx.call(sink_proxies.$idx);
                    )+

                    (($([<source $idx>],)+), async move {
                        ($([<fut $idx>],)+).join().await;
                    })
                }
            }
        }
    };
}

impl_drivers!((0, (T1, D1)));
impl_drivers!((0, (T1, D1)), (1, (T2, D2)));
impl_drivers!((0, (T1, D1)), (1, (T2, D2)), (2, (T3, D3)));
impl_drivers!((0, (T1, D1)), (1, (T2, D2)), (2, (T3, D3)), (3, (T4, D4)));
impl_drivers!(
    (0, (T1, D1)),
    (1, (T2, D2)),
    (2, (T3, D3)),
    (3, (T4, D4)),
    (4, (T5, D5))
);
impl_drivers!(
    (0, (T1, D1)),
    (1, (T2, D2)),
    (2, (T3, D3)),
    (3, (T4, D4)),
    (4, (T5, D5)),
    (5, (T6, D6))
);
impl_drivers!(
    (0, (T1, D1)),
    (1, (T2, D2)),
    (2, (T3, D3)),
    (3, (T4, D4)),
    (4, (T5, D5)),
    (5, (T6, D6)),
    (6, (T7, D7))
);
impl_drivers!(
    (0, (T1, D1)),
    (1, (T2, D2)),
    (2, (T3, D3)),
    (3, (T4, D4)),
    (4, (T5, D5)),
    (5, (T6, D6)),
    (6, (T7, D7)),
    (7, (T8, D8))
);
impl_drivers!(
    (0, (T1, D1)),
    (1, (T2, D2)),
    (2, (T3, D3)),
    (3, (T4, D4)),
    (4, (T5, D5)),
    (5, (T6, D6)),
    (6, (T7, D7)),
    (7, (T8, D8)),
    (8, (T9, D9))
);
impl_drivers!(
    (0, (T1, D1)),
    (1, (T2, D2)),
    (2, (T3, D3)),
    (3, (T4, D4)),
    (4, (T5, D5)),
    (5, (T6, D6)),
    (6, (T7, D7)),
    (7, (T8, D8)),
    (8, (T9, D9)),
    (9, (T10, D10))
);
impl_drivers!(
    (0, (T1, D1)),
    (1, (T2, D2)),
    (2, (T3, D3)),
    (3, (T4, D4)),
    (4, (T5, D5)),
    (5, (T6, D6)),
    (6, (T7, D7)),
    (7, (T8, D8)),
    (8, (T9, D9)),
    (9, (T10, D10)),
    (10, (T11, D11))
);
impl_drivers!(
    (0, (T1, D1)),
    (1, (T2, D2)),
    (2, (T3, D3)),
    (3, (T4, D4)),
    (4, (T5, D5)),
    (5, (T6, D6)),
    (6, (T7, D7)),
    (7, (T8, D8)),
    (8, (T9, D9)),
    (9, (T10, D10)),
    (10, (T11, D11)),
    (11, (T12, D12))
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
        $(($idx:tt, $sink:ident)),+
    ) => {
        impl<$($sink,)+> Sinks for ($($sink,)+)
        where
            $($sink: Stream + Send,)+
            $($sink::Item: Send,)+
        {
            type SinkReceivers = ($(ReceiverStream<$sink::Item>,)+);
            type SinkSenders = ($(mpsc::Sender<$sink::Item>,)+);

            fn make_sink_proxies() -> (Self::SinkSenders, Self::SinkReceivers) {
                paste! {
                    $(
                        let ([<tx $idx>], [<rx $idx>]) = mpsc::channel(SINK_PROXY_BUFFER_LEN);
                    )+

                    (($([<tx $idx>],)+), ($(ReceiverStream::new([<rx $idx>]),)+))
                }
            }

            #[allow(
                refining_impl_trait,
                reason = "the `Send` bound is not placed on the trait method, in order to allow \
                          implementing the `Sinks` trait for `!Send` types"
            )]
            #[allow(
                clippy::manual_async_fn,
                reason = "warning: use of `async fn` in public traits is discouraged as auto trait \
                          bounds cannot be specified"
            )]
            fn replicate_many(
                self,
                sink_senders: Self::SinkSenders,
            ) -> impl Future<Output = ()> + Send {
                async move {
                    ($(self.$idx.then(|x| {
                        let tx = sink_senders.$idx.clone();
                        async move {
                            let permit = tx.reserve().await.unwrap();
                            permit.send(x);
                        }
                    }),)+)
                        .merge()
                        .last()
                        .await;
                }
            }
        }
    };
}

impl_sinks!((0, Sink1));
impl_sinks!((0, Sink1), (1, Sink2));
impl_sinks!((0, Sink1), (1, Sink2), (2, Sink3));
impl_sinks!((0, Sink1), (1, Sink2), (2, Sink3), (3, Sink4));
impl_sinks!((0, Sink1), (1, Sink2), (2, Sink3), (3, Sink4), (4, Sink5));
impl_sinks!(
    (0, Sink1),
    (1, Sink2),
    (2, Sink3),
    (3, Sink4),
    (4, Sink5),
    (5, Sink6)
);
impl_sinks!(
    (0, Sink1),
    (1, Sink2),
    (2, Sink3),
    (3, Sink4),
    (4, Sink5),
    (5, Sink6),
    (6, Sink7)
);
impl_sinks!(
    (0, Sink1),
    (1, Sink2),
    (2, Sink3),
    (3, Sink4),
    (4, Sink5),
    (5, Sink6),
    (6, Sink7),
    (7, Sink8)
);
impl_sinks!(
    (0, Sink1),
    (1, Sink2),
    (2, Sink3),
    (3, Sink4),
    (4, Sink5),
    (5, Sink6),
    (6, Sink7),
    (7, Sink8),
    (8, Sink9)
);
impl_sinks!(
    (0, Sink1),
    (1, Sink2),
    (2, Sink3),
    (3, Sink4),
    (4, Sink5),
    (5, Sink6),
    (6, Sink7),
    (7, Sink8),
    (8, Sink9),
    (9, Sink10)
);
impl_sinks!(
    (0, Sink1),
    (1, Sink2),
    (2, Sink3),
    (3, Sink4),
    (4, Sink5),
    (5, Sink6),
    (6, Sink7),
    (7, Sink8),
    (8, Sink9),
    (9, Sink10),
    (10, Sink11)
);
impl_sinks!(
    (0, Sink1),
    (1, Sink2),
    (2, Sink3),
    (3, Sink4),
    (4, Sink5),
    (5, Sink6),
    (6, Sink7),
    (7, Sink8),
    (8, Sink9),
    (9, Sink10),
    (10, Sink11),
    (11, Sink12)
);

pub fn setup<M, Drv, Src, Snk, DrvIn>(
    main: M,
    drivers: Drv,
) -> (/* Src, Snk, */ impl AsyncFnOnce(),)
where
    M: Main<Src, Snk>,
    Drv: Drivers<DrvIn, Sources = Src>,
    Src: Sources,
    Snk: Sinks<SinkReceivers = Drv::SinkProxies>,
{
    let (sources, run) = setup_reusable(drivers);
    let sinks = main.call(sources);
    let run = async move || run(sinks).await;

    (/* sources, sinks, */ run,)
}

pub fn setup_reusable<Drv, Src, Snk, DrvIn>(drivers: Drv) -> (Src, impl AsyncFnOnce(Snk))
where
    Drv: Drivers<DrvIn, Sources = Src>,
    Src: Sources,
    Snk: Sinks<SinkReceivers = Drv::SinkProxies>,
{
    let (sink_senders, sink_proxies) = Snk::make_sink_proxies();
    let (sources, drivers_fut) = drivers.call(sink_proxies);

    (sources, async |sinks: Snk| {
        (sinks.replicate_many(sink_senders), drivers_fut)
            .join()
            .await;
    })
}

pub async fn run<M, Drv, Src, Snk, DrvIn>(main: M, drivers: Drv)
where
    M: Main<Src, Snk>,
    Drv: Drivers<DrvIn, Sources = Src>,
    Src: Sources,
    Snk: Sinks<SinkReceivers = Drv::SinkProxies>,
{
    let (/* _sources, _sinks, */ run,) = setup(main, drivers);

    run().await
}
