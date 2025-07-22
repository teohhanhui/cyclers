use futures_core::Stream;

use crate::BoxError;

#[cfg(feature = "console")]
pub mod console;
#[cfg(feature = "webrtc")]
pub mod webrtc;

pub trait Driver<Sink>
where
    Sink: Stream,
{
    type Input;
    type Source: Source;

    fn call(self, sink: Sink) -> (Self::Source, impl Future<Output = Result<(), BoxError>>);
}

pub trait Source {}

pub trait MakeDriver<Sink>
where
    Sink: Stream,
{
    type Driver: Driver<Sink>;

    fn call(&mut self) -> Self::Driver;
}
