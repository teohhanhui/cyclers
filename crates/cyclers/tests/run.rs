use std::marker::PhantomData;

use anyhow::Result;
use cyclers::BoxError;
use cyclers::driver::{Driver, Source};
use futures_lite::{Stream, future, stream};
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use tokio::test;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen_test::wasm_bindgen_test as test;

#[test]
async fn it_connects_main_and_drivers() -> Result<()> {
    struct MockDriver;

    struct MockSource<Sink>
    where
        Sink: Stream,
    {
        sink: PhantomData<Sink>,
    }

    enum MockCommand {}

    impl<Sink> Driver<Sink> for MockDriver
    where
        Sink: Stream<Item = MockCommand>,
    {
        type Input = MockCommand;
        type Source = MockSource<Sink>;

        fn call(self, _sink: Sink) -> (Self::Source, impl Future<Output = Result<(), BoxError>>) {
            (MockSource { sink: PhantomData }, future::ready(Ok(())))
        }
    }

    impl<Sink> Source for MockSource<Sink> where Sink: Stream {}

    cyclers::run(
        |_mock_source: MockSource<_>| {
            let mock_sink = stream::empty::<Result<MockCommand, anyhow::Error>>();

            (mock_sink,)
        },
        (MockDriver,),
    )
    .await
    .map_err(anyhow::Error::from_boxed)
}
