use anyhow::Result;
use cyclers::driver::Driver as _;
use cyclers_http::{ClientBuilder, HttpCommand, HttpDriver, HttpSource};
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

    fut.await.map_err(anyhow::Error::from_boxed)?;

    {
        let response = response.next().await.unwrap()?;
        assert_eq!(response.body(), "world".as_bytes());
        api_mock.assert();
    }

    assert!(response.next().await.is_none());

    Ok(())
}
