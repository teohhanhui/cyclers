use std::error::Error;
use std::sync::Arc;

use futures_concurrency::future::TryJoin as _;
use futures_concurrency::stream::Merge as _;
use futures_lite::{Stream, StreamExt as _, pin, stream};
use paste::paste;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::driver::{Driver, Source};
use crate::util::{head, head_or_tail};

/// Buffer capacity for the proxy channel created for each sink stream.
///
/// Since the sink stream is strictly pull-based, the buffer capacity should
/// always be 1.
///
/// See <https://github.com/rust-lang/futures-rs/issues/69#issuecomment-240434585>
const SINK_PROXY_BUFFER_LEN: usize = 1;

/// Type alias for a type-erased error type.
pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

/// Type alias for a type-erased error type that can be cloned.
pub type ArcError = Arc<dyn Error + Send + Sync + 'static>;

/// A [pure] `main` function.
///
/// [pure]: https://en.wikipedia.org/wiki/Pure_function
pub trait Main<Sources, Sinks> {
    /// Calls the `main` function with a collection of source objects from each
    /// driver. Returns a collection of sink streams for each driver.
    fn call(self, sources: Sources) -> Sinks;
}

/// A heterogeneous collection of driver functions.
pub trait Drivers<Sinks> {
    type Sources: Sources;
    type Termination;

    /// Calls each driver function. Returns a collection of source objects from
    /// each driver, and an aggregate run loop future for the drivers.
    ///
    /// The returned aggregate run loop future must be polled for the drivers to
    /// perform unobserved side effects (i.e. not queried through the source
    /// objects).
    fn call(
        self,
        sinks: Sinks,
    ) -> (
        Self::Sources,
        impl Future<Output = Result<Self::Termination, BoxError>>,
    );
}

/// A heterogeneous collection of source objects.
pub trait Sources {}

/// A heterogeneous collection of sink streams.
pub trait Sinks {
    type ProxyReceivers;
    type ProxySenders;

    /// Creates a proxy channel for each sink stream. Returns a collection of
    /// senders for each proxy channel, and a collection of receivers for each
    /// proxy channel.
    fn make_proxy_channels() -> (Self::ProxySenders, Self::ProxyReceivers);

    /// Subscribes to each sink stream. Each item yielded by the sink stream is
    /// passed on to the corresponding driver's sink, by sending it through the
    /// proxy channel.
    ///
    /// If the receiving end of the proxy channel has been disconnected, it
    /// means the driver has exited, so it is safe to stop proxying for that
    /// sink stream as well.
    fn proxy(
        self,
        proxy_senders: Self::ProxySenders,
    ) -> impl Future<Output = Result<(), BoxError>> + MaybeSend;
}

#[cfg(not(target_family = "wasm"))]
pub trait MaybeSend: Send {}
#[cfg(target_family = "wasm")]
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

impl_main!((0, (Src1, Snk1)));
impl_main!((0, (Src1, Snk1)), (1, (Src2, Snk2)));
impl_main!((0, (Src1, Snk1)), (1, (Src2, Snk2)), (2, (Src3, Snk3)));
impl_main!(
    (0, (Src1, Snk1)),
    (1, (Src2, Snk2)),
    (2, (Src3, Snk3)),
    (3, (Src4, Snk4))
);
impl_main!(
    (0, (Src1, Snk1)),
    (1, (Src2, Snk2)),
    (2, (Src3, Snk3)),
    (3, (Src4, Snk4)),
    (4, (Src5, Snk5))
);
impl_main!(
    (0, (Src1, Snk1)),
    (1, (Src2, Snk2)),
    (2, (Src3, Snk3)),
    (3, (Src4, Snk4)),
    (4, (Src5, Snk5)),
    (5, (Src6, Snk6))
);
impl_main!(
    (0, (Src1, Snk1)),
    (1, (Src2, Snk2)),
    (2, (Src3, Snk3)),
    (3, (Src4, Snk4)),
    (4, (Src5, Snk5)),
    (5, (Src6, Snk6)),
    (6, (Src7, Snk7))
);
impl_main!(
    (0, (Src1, Snk1)),
    (1, (Src2, Snk2)),
    (2, (Src3, Snk3)),
    (3, (Src4, Snk4)),
    (4, (Src5, Snk5)),
    (5, (Src6, Snk6)),
    (6, (Src7, Snk7)),
    (7, (Src8, Snk8))
);
impl_main!(
    (0, (Src1, Snk1)),
    (1, (Src2, Snk2)),
    (2, (Src3, Snk3)),
    (3, (Src4, Snk4)),
    (4, (Src5, Snk5)),
    (5, (Src6, Snk6)),
    (6, (Src7, Snk7)),
    (7, (Src8, Snk8)),
    (8, (Src9, Snk9))
);
impl_main!(
    (0, (Src1, Snk1)),
    (1, (Src2, Snk2)),
    (2, (Src3, Snk3)),
    (3, (Src4, Snk4)),
    (4, (Src5, Snk5)),
    (5, (Src6, Snk6)),
    (6, (Src7, Snk7)),
    (7, (Src8, Snk8)),
    (8, (Src9, Snk9)),
    (9, (Src10, Snk10))
);
impl_main!(
    (0, (Src1, Snk1)),
    (1, (Src2, Snk2)),
    (2, (Src3, Snk3)),
    (3, (Src4, Snk4)),
    (4, (Src5, Snk5)),
    (5, (Src6, Snk6)),
    (6, (Src7, Snk7)),
    (7, (Src8, Snk8)),
    (8, (Src9, Snk9)),
    (9, (Src10, Snk10)),
    (10, (Src11, Snk11))
);
impl_main!(
    (0, (Src1, Snk1)),
    (1, (Src2, Snk2)),
    (2, (Src3, Snk3)),
    (3, (Src4, Snk4)),
    (4, (Src5, Snk5)),
    (5, (Src6, Snk6)),
    (6, (Src7, Snk7)),
    (7, (Src8, Snk8)),
    (8, (Src9, Snk9)),
    (9, (Src10, Snk10)),
    (10, (Src11, Snk11)),
    (11, (Src12, Snk12))
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
            type Termination = head!($($driver::Termination),+);

            fn call(
                self,
                sinks: ($($sink,)+),
            ) -> (Self::Sources, impl Future<Output = Result<Self::Termination, BoxError>>) {
                paste! {
                    $(
                        let ([<source $idx>], [<fut $idx>]) = self.$idx.call(sinks.$idx);
                    )+

                    let aggregate_run = async move {
                        // Polls the run loop futures from each driver, until the first driver
                        // exits. Yields the termination value from the first driver.
                        //
                        // The termination values from the other drivers are always discarded.
                        let s = head_or_tail!(
                            $(
                                (
                                    stream::once_future([<fut $idx>]).map(|out| out.map(Some)),
                                    stream::once_future([<fut $idx>]).map(|out| out.map(|_| None))
                                )
                            ),+
                        )
                            .merge();
                        pin!(s);

                        while let Some(out) = s.try_next().await? {
                            // If the item is `Some(_)`, it is the termination value from the first
                            // driver.
                            if let Some(out) = out {
                                return Ok(out);
                            }
                        }
                        unreachable!();
                    };

                    (($([<source $idx>],)+), aggregate_run)
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

impl_sources!(S1);
impl_sources!(S1, S2);
impl_sources!(S1, S2, S3);
impl_sources!(S1, S2, S3, S4);
impl_sources!(S1, S2, S3, S4, S5);
impl_sources!(S1, S2, S3, S4, S5, S6);
impl_sources!(S1, S2, S3, S4, S5, S6, S7);
impl_sources!(S1, S2, S3, S4, S5, S6, S7, S8);
impl_sources!(S1, S2, S3, S4, S5, S6, S7, S8, S9);
impl_sources!(S1, S2, S3, S4, S5, S6, S7, S8, S9, S10);
impl_sources!(S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11);
impl_sources!(S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11, S12);

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
            type ProxyReceivers = ($(ReceiverStream<$t>,)+);
            type ProxySenders = ($(mpsc::Sender<$t>,)+);

            fn make_proxy_channels() -> (Self::ProxySenders, Self::ProxyReceivers) {
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
            fn proxy(
                self,
                proxy_senders: Self::ProxySenders,
            ) -> impl Future<Output = Result<(), BoxError>> + MaybeSend {
                async move {
                    let s = (
                        $(
                            stream::unfold(
                                (Box::pin(self.$idx), proxy_senders.$idx),
                                async move |(mut sink, tx)| {
                                    let x = match sink.next().await? {
                                        Ok(x) => x,
                                        Err(err) => return Some((Err(err.into()), (sink, tx))),
                                    };
                                    let permit = tx.reserve().await.ok()?;
                                    permit.send(x);

                                    Some((Ok(()), (sink, tx)))
                                },
                            ),
                        )+
                    )
                        .merge();
                    pin!(s);

                    while s.try_next().await?.is_some() {}
                    Ok(())
                }
            }
        }
    };
}

impl_sinks!((0, S1, T1, E1));
impl_sinks!((0, S1, T1, E1), (1, S2, T2, E2));
impl_sinks!((0, S1, T1, E1), (1, S2, T2, E2), (2, S3, T3, E3));
impl_sinks!(
    (0, S1, T1, E1),
    (1, S2, T2, E2),
    (2, S3, T3, E3),
    (3, S4, T4, E4)
);
impl_sinks!(
    (0, S1, T1, E1),
    (1, S2, T2, E2),
    (2, S3, T3, E3),
    (3, S4, T4, E4),
    (4, S5, T5, E5)
);
impl_sinks!(
    (0, S1, T1, E1),
    (1, S2, T2, E2),
    (2, S3, T3, E3),
    (3, S4, T4, E4),
    (4, S5, T5, E5),
    (5, S6, T6, E6)
);
impl_sinks!(
    (0, S1, T1, E1),
    (1, S2, T2, E2),
    (2, S3, T3, E3),
    (3, S4, T4, E4),
    (4, S5, T5, E5),
    (5, S6, T6, E6),
    (6, S7, T7, E7)
);
impl_sinks!(
    (0, S1, T1, E1),
    (1, S2, T2, E2),
    (2, S3, T3, E3),
    (3, S4, T4, E4),
    (4, S5, T5, E5),
    (5, S6, T6, E6),
    (6, S7, T7, E7),
    (7, S8, T8, E8)
);
impl_sinks!(
    (0, S1, T1, E1),
    (1, S2, T2, E2),
    (2, S3, T3, E3),
    (3, S4, T4, E4),
    (4, S5, T5, E5),
    (5, S6, T6, E6),
    (6, S7, T7, E7),
    (7, S8, T8, E8),
    (8, S9, T9, E9)
);
impl_sinks!(
    (0, S1, T1, E1),
    (1, S2, T2, E2),
    (2, S3, T3, E3),
    (3, S4, T4, E4),
    (4, S5, T5, E5),
    (5, S6, T6, E6),
    (6, S7, T7, E7),
    (7, S8, T8, E8),
    (8, S9, T9, E9),
    (9, S10, T10, E10)
);
impl_sinks!(
    (0, S1, T1, E1),
    (1, S2, T2, E2),
    (2, S3, T3, E3),
    (3, S4, T4, E4),
    (4, S5, T5, E5),
    (5, S6, T6, E6),
    (6, S7, T7, E7),
    (7, S8, T8, E8),
    (8, S9, T9, E9),
    (9, S10, T10, E10),
    (10, S11, T11, E11)
);
impl_sinks!(
    (0, S1, T1, E1),
    (1, S2, T2, E2),
    (2, S3, T3, E3),
    (3, S4, T4, E4),
    (4, S5, T5, E5),
    (5, S6, T6, E6),
    (6, S7, T7, E7),
    (7, S8, T8, E8),
    (8, S9, T9, E9),
    (9, S10, T10, E10),
    (10, S11, T11, E11),
    (11, S12, T12, E12)
);

#[cfg(not(target_family = "wasm"))]
impl<T: Send> MaybeSend for T {}
#[cfg(target_family = "wasm")]
impl<T> MaybeSend for T {}

/// Prepares the application to be executed.
///
/// Takes a `main` function and prepares to circularly connect it to the given
/// collection of driver functions. Returns a `run` function. Only when the
/// `run` function is called will the application actually execute.
pub fn setup<M, Drv, Snk>(
    main: M,
    drivers: Drv,
) -> (impl AsyncFnOnce() -> Result<Drv::Termination, BoxError>,)
where
    M: Main<Drv::Sources, Snk>,
    Drv: Drivers<Snk::ProxyReceivers>,
    Snk: Sinks,
{
    let (sources, run) = setup_partial(drivers);
    let sinks = main.call(sources);
    let run = async move || run(sinks).await;

    (run,)
}

/// A partially-applied variant of [`setup`] which accepts only the drivers.
///
/// Takes a collection of driver functions as input, and returns a collection of
/// the generated sources (from those drivers), and a `run` function (which in
/// turn expects sinks as argument).
///
/// `run` is the function that once called with `sinks` as argument, will
/// execute the application, tying together sources with sinks.
pub fn setup_partial<Drv, Snk>(
    drivers: Drv,
) -> (
    Drv::Sources,
    impl AsyncFnOnce(Snk) -> Result<Drv::Termination, BoxError>,
)
where
    Drv: Drivers<Snk::ProxyReceivers>,
    Snk: Sinks,
{
    let (proxy_senders, proxy_receivers) = Snk::make_proxy_channels();
    let (sources, drivers_fut) = drivers.call(proxy_receivers);

    let run = async |sinks: Snk| {
        let (out, _) = (drivers_fut, sinks.proxy(proxy_senders)).try_join().await?;

        Ok(out)
    };

    (sources, run)
}

/// Takes a `main` function and circularly connects it to the given collection
/// of driver functions.
///
/// The `main` function expects a collection of "source" objects (returned from
/// drivers) as input, and should return a collection of "sink" streams (to be
/// given to drivers).
pub async fn run<M, Drv, Snk>(main: M, drivers: Drv) -> Result<Drv::Termination, BoxError>
where
    M: Main<Drv::Sources, Snk>,
    Drv: Drivers<Snk::ProxyReceivers>,
    Snk: Sinks,
{
    let (run,) = setup(main, drivers);

    run().await
}
