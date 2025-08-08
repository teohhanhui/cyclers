use futures_lite::Stream;

use crate::BoxError;

/// A [pure] driver function.
///
/// [pure]: https://en.wikipedia.org/wiki/Pure_function
pub trait Driver<Sink>
where
    Sink: Stream,
{
    type Source: Source;
    type Termination;

    /// Calls the driver function with a sink stream. Returns a source object,
    /// and a run loop future.
    ///
    /// The returned run loop future must be polled for the driver to perform
    /// unobserved side effects (i.e. not queried through the source object).
    fn call(
        self,
        sink: Sink,
    ) -> (
        Self::Source,
        impl Future<Output = Result<Self::Termination, BoxError>>,
    );
}

/// A source object that provides methods to query for source streams.
pub trait Source {}

pub trait MakeDriver<Sink>
where
    Sink: Stream,
{
    type Driver: Driver<Sink>;

    fn call(&mut self) -> Self::Driver;
}
