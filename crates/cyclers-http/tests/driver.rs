#[cfg(not(target_family = "wasm"))]
use std::time::Duration;

#[cfg(not(target_family = "wasm"))]
use anyhow::Context as _;
use anyhow::Result;
use cyclers::driver::Driver as _;
use cyclers_http::{ClientBuilder, HttpCommand, HttpDriver, HttpSource};
#[cfg(not(target_family = "wasm"))]
use futures_concurrency::future::TryJoin as _;
use futures_lite::stream;
#[cfg(not(target_family = "wasm"))]
use futures_lite::{StreamExt as _, pin};
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
#[cfg(not(target_family = "wasm"))]
async fn it_receives_responses() -> Result<()> {
    let server = httpmock::MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.path("/hello");
            then.status(200).body("world");
        })
        .await;

    let sink = stream::iter([
        HttpCommand::from(ClientBuilder::new()),
        HttpCommand::from(
            http::Request::builder()
                .method("GET")
                .uri(server.url("/hello"))
                .body("".into())?,
        ),
    ]);
    let (source, fut): (HttpSource<_>, _) = HttpDriver.call(sink);

    let responses = source
        .responses()
        .map(|res| res.map_err(anyhow::Error::from_boxed));
    pin!(responses);

    (fut, async move {
        let response = responses
            .try_next()
            .await?
            .context("no responses were received")?;
        assert_eq!(response.body(), "world");
        mock.assert();
        Ok(())
    })
        .try_join()
        .await
        .map(|_| ())
        .map_err(anyhow::Error::from_boxed)
}

#[test]
#[cfg(not(target_family = "wasm"))]
async fn it_handles_concurrent_requests() -> Result<()> {
    let server = httpmock::MockServer::start_async().await;
    let mock1 = server
        .mock_async(|when, then| {
            when.path("/hello");
            then.status(200).body("world").delay({
                // Make sure the response for the first request will not yet be received by the
                // time the second request is supposed to be sent out.
                Duration::from_secs(1)
            });
        })
        .await;
    let mock2 = server
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
                .body("".into())?,
        ),
        HttpCommand::from(
            http::Request::builder()
                .method("GET")
                .uri(server.url("/foo"))
                .body("".into())?,
        ),
    ]);
    let (source, fut): (HttpSource<_>, _) = HttpDriver.call(sink);

    let responses = source
        .responses()
        .map(|res| res.map_err(anyhow::Error::from_boxed));
    pin!(responses);

    (fut, async {
        let response = responses
            .try_next()
            .await?
            .context("no responses were received")?;
        assert_eq!(response.body(), "bar");
        mock1.assert();
        mock2.assert();
        Ok(())
    })
        .try_join()
        .await
        .map(|_| ())
        .map_err(anyhow::Error::from_boxed)
}
