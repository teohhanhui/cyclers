use std::time::Duration;

use anyhow::Result;
use cyclers::driver::Driver as _;
use cyclers_http::{ClientBuilder, HttpCommand, HttpDriver, HttpSource};
use futures_concurrency::future::TryJoin as _;
use futures_lite::{StreamExt as _, pin, stream};
#[cfg(not(target_family = "wasm"))]
use tokio::test;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use wasm_bindgen_test::wasm_bindgen_test as test;
#[cfg(all(target_os = "wasi", target_env = "p2"))]
use wstd::test;

#[test]
async fn it_returns_source_when_called_with_sink() {
    let sink = stream::pending::<HttpCommand>();
    let (_source, _fut): (HttpSource<_>, _) = HttpDriver.call(sink);
}

#[test]
async fn it_exits_when_the_sink_stream_has_finished() -> Result<()> {
    let sink = stream::empty::<HttpCommand>();
    let (_source, fut): (HttpSource<_>, _) = HttpDriver.call(sink);
    fut.await.map_err(anyhow::Error::from_boxed)
}

#[test]
async fn it_allows_configuring_client() -> Result<()> {
    let sink = stream::once(HttpCommand::from(ClientBuilder::new()));
    let (_source, fut): (HttpSource<_>, _) = HttpDriver.call(sink);
    fut.await.map_err(anyhow::Error::from_boxed)
}

#[test]
async fn it_receives_responses() -> Result<()> {
    let server = httpmock::MockServer::start();
    let api_mock = server.mock(|when, then| {
        when.path("/hello");
        then.status(200).body("world");
    });

    let sink = stream::iter([
        HttpCommand::from(ClientBuilder::new()),
        HttpCommand::from(
            http::Request::builder()
                .method("GET")
                .uri(server.url("/hello"))
                .body(vec![].into())?,
        ),
    ]);
    let (source, fut): (HttpSource<_>, _) = HttpDriver.call(sink);

    let response = source
        .responses()
        .map(|res| res.map_err(anyhow::Error::from_boxed));
    pin!(response);

    (fut, async move {
        let response = response.try_next().await?.unwrap();
        assert_eq!(response.body(), "world".as_bytes());
        api_mock.assert();
        Ok(())
    })
        .try_join()
        .await
        .map(|_| ())
        .map_err(anyhow::Error::from_boxed)
}

#[test]
async fn it_handles_concurrent_requests() -> Result<()> {
    let server = httpmock::MockServer::start_async().await;
    let hello_mock = server
        .mock_async(|when, then| {
            when.path("/hello");
            then
                .status(200)
                .body("world")
                // Simulates a network delay
                .delay(Duration::from_secs(1));
        })
        .await;
    let foobar_mock = server
        .mock_async(|when, then| {
            when.path("/foo");
            then.status(200).body("bar");
        })
        .await;

    let sink = stream::iter([
        HttpCommand::from(
            http::Request::builder()
                .method("GET")
                .uri(server.url("/hello"))
                .body(vec![].into())?,
        ),
        HttpCommand::from(
            http::Request::builder()
                .method("GET")
                .uri(server.url("/foo"))
                .body(vec![].into())?,
        ),
    ]);
    let (source, fut): (HttpSource<_>, _) = HttpDriver.call(sink);

    let response = source
        .responses()
        .map(|res| res.map_err(anyhow::Error::from_boxed));
    pin!(response);

    (fut, async {
        let response = response.try_next().await?.unwrap();
        assert_eq!(response.body(), "bar".as_bytes());
        hello_mock.assert();
        foobar_mock.assert();
        Ok(())
    })
        .try_join()
        .await
        .map(|_| ())
        .map_err(anyhow::Error::from_boxed)
}
