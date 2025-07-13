use futures_concurrency::future::Join as _;
use futures_concurrency::stream::Merge as _;
use futures_core::Stream;
use futures_lite::StreamExt as _;
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

    async fn replicate_many(self, sink_senders: Self::SinkSenders);
}

impl<F, Source1, Sink1> Main<(Source1,), (Sink1,)> for F
where
    F: FnOnce(Source1) -> (Sink1,),
    Source1: Source,
    Sink1: Stream,
{
    fn call(self, sources: (Source1,)) -> (Sink1,) {
        self(sources.0)
    }
}

impl<T1, D1> Drivers<(T1,)> for (D1,)
where
    D1: Driver<ReceiverStream<T1>>,
{
    type SinkProxies = (ReceiverStream<T1>,);
    type Sources = (D1::Source,);

    fn call(self, sink_proxies: Self::SinkProxies) -> (Self::Sources, impl Future<Output = ()>) {
        let (source1, fut1) = self.0.call(sink_proxies.0);

        ((source1,), async move {
            (fut1,).join().await;
        })
    }
}

impl<Source1> Sources for (Source1,) where Source1: Source {}

impl<Sink1> Sinks for (Sink1,)
where
    Sink1: Stream,
{
    type SinkReceivers = (ReceiverStream<Sink1::Item>,);
    type SinkSenders = (mpsc::Sender<Sink1::Item>,);

    fn make_sink_proxies() -> (Self::SinkSenders, Self::SinkReceivers) {
        let (tx1, rx1) = mpsc::channel(SINK_PROXY_BUFFER_LEN);

        ((tx1,), (ReceiverStream::new(rx1),))
    }

    async fn replicate_many(self, sink_senders: Self::SinkSenders) {
        (self.0.then(|x| {
            let tx = sink_senders.0.clone();
            async move {
                let permit = tx.reserve().await.unwrap();
                permit.send(x);
            }
        }),)
            .merge()
            .last()
            .await;
    }
}

pub fn setup<M, Drv, Src, Snk, DrvIn>(
    main: M,
    drivers: Drv,
) -> (/* Src, */ Snk, impl AsyncFnOnce(Snk))
where
    M: Main<Src, Snk>,
    Drv: Drivers<DrvIn, Sources = Src>,
    Src: Sources,
    Snk: Sinks<SinkReceivers = Drv::SinkProxies>,
{
    let (sources, run) = setup_reusable(drivers);
    let sinks = main.call(sources);

    (/* sources, */ sinks, run)
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
    let (/* _sources, */ sinks, run) = setup(main, drivers);

    run(sinks).await
}
