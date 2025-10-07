#[cfg(not(target_family = "wasm"))]
use std::sync::Arc;

use anyhow::Result;
#[cfg(not(target_family = "wasm"))]
use anyhow::{Context as _, anyhow};
use cyclers::driver::Driver as _;
#[cfg(not(target_family = "wasm"))]
use cyclers::{ArcError, BoxError};
#[cfg(not(target_family = "wasm"))]
use cyclers_webtrans::command::{
    ConfigureClientCommand, CreateBiStreamCommand, EstablishSessionCommand, SendBytesCommand,
};
use cyclers_webtrans::{WebTransportCommand, WebTransportDriver, WebTransportSource};
#[cfg(not(target_family = "wasm"))]
use futures_concurrency::future::TryJoin as _;
#[cfg(not(target_family = "wasm"))]
use futures_concurrency::stream::{Chain as _, Merge as _};
use futures_lite::stream;
#[cfg(not(target_family = "wasm"))]
use futures_lite::{StreamExt as _, pin};
#[cfg(not(target_family = "wasm"))]
use futures_rx::RxExt as _;
#[cfg(not(target_family = "wasm"))]
use rcgen::{CertifiedKey, generate_simple_self_signed};
#[cfg(not(target_family = "wasm"))]
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
#[cfg(not(target_family = "wasm"))]
use tokio::sync::mpsc;
#[cfg(not(target_family = "wasm"))]
use tokio::test;
#[cfg(not(target_family = "wasm"))]
use tokio_stream::wrappers::ReceiverStream;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use wasm_bindgen_test::wasm_bindgen_test as test;
#[cfg(not(target_family = "wasm"))]
use web_transport_quinn::{ClientBuilder, ServerBuilder, quinn::rustls};

#[test]
async fn it_returns_source_when_called_with_sink() {
    let sink = stream::pending::<WebTransportCommand>();
    let (_source, _fut): (WebTransportSource<_>, _) = WebTransportDriver.call(sink);
}

#[test]
async fn it_exits_when_the_sink_stream_has_finished() -> Result<()> {
    let sink = stream::empty::<WebTransportCommand>();
    let (_source, fut): (WebTransportSource<_>, _) = WebTransportDriver.call(sink);
    fut.await.map_err(anyhow::Error::from_boxed)
}

#[cfg(not(target_family = "wasm"))]
#[test]
#[test_log::test]
async fn it_sends_and_receives_bytes() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let (cert, key): (CertificateDer<'_>, PrivateKeyDer) = {
        let CertifiedKey { cert, signing_key } =
            generate_simple_self_signed(["localhost".to_owned()])?;
        (
            cert.into(),
            signing_key
                .serialize_der()
                .try_into()
                .map_err(|err: &str| anyhow!(err))?,
        )
    };
    let mut server = ServerBuilder::new()
        .with_addr("[::1]:4443".parse()?)
        .with_certificate(vec![cert.clone()], key)?;

    let (sink_tx, sink_rx) = mpsc::channel(1);
    let (source, fut): (WebTransportSource<_>, _) =
        WebTransportDriver.call(ReceiverStream::new(sink_rx));

    let configure_client = stream::once(WebTransportCommand::from(ConfigureClientCommand::from(
        || ClientBuilder::new().with_server_certificates(vec![cert]),
    )));

    let establish_session = stream::once(WebTransportCommand::from(EstablishSessionCommand {
        url: "https://localhost:4443".try_into()?,
    }));

    let sessions = source
        .session_ready()
        .take(1)
        .map(|session| session.map_err(|err| ArcError::from(BoxError::from(err))))
        .share();
    let establish_session_errors = sessions
        .clone()
        .filter_map(|session| session.as_ref().err().map(|err| Err(Arc::clone(err))));
    let session_ids =
        sessions.filter_map(|session| session.as_ref().ok().map(|(session_id, _url)| *session_id));

    let create_bi_stream = session_ids.clone().map(|session_id| {
        WebTransportCommand::from(CreateBiStreamCommand {
            session_id,
            send_order: Default::default(),
        })
    });

    let streams = session_ids
        .flat_map(|session_id| source.bi_stream_created(session_id))
        .take(1)
        .map(|stream| stream.map_err(|err| ArcError::from(BoxError::from(err))))
        .share();
    let create_stream_errors = streams
        .clone()
        .filter_map(|stream| stream.as_ref().err().map(|err| Err(Arc::clone(err))));
    let stream_ids = streams.filter_map(|stream| stream.as_ref().ok().cloned());

    let send_bytes = stream_ids.clone().map(|stream_id| {
        WebTransportCommand::from(SendBytesCommand {
            stream_id,
            bytes: b"hello"[..].into(),
            finished: true,
        })
    });

    let bytes_received = stream_ids
        .flat_map(|stream_id| source.bytes_received(stream_id))
        .map(|res| res.map_err(|err| ArcError::from(BoxError::from(err))));
    pin!(bytes_received);

    let sink = (
        (configure_client.map(Ok), establish_session.map(Ok)).chain(),
        create_bi_stream.map(Ok),
        send_bytes.map(Ok),
        establish_session_errors,
        create_stream_errors,
    )
        .merge();
    pin!(sink);

    (
        async move {
            let request = server
                .accept()
                .await
                .ok_or_else(|| anyhow!("no requests were received"))?;
            let session = request.ok().await?;
            let (mut tx, mut rx) = session.accept_bi().await?;
            let bytes = rx.read_to_end(1024).await?;
            assert_eq!(&bytes[..], b"hello");
            tx.write_all(&bytes).await?;
            Ok(())
        },
        fut,
        async move {
            while let Some(command) = sink.try_next().await? {
                let permit = sink_tx.reserve().await?;
                permit.send(command);
            }
            Ok(())
        },
        async move {
            let bytes = bytes_received
                .try_next()
                .await?
                .context("no bytes were received")?;
            assert_eq!(&bytes[..], b"hello");
            Ok(())
        },
    )
        .try_join()
        .await
        .map(|_| ())
        .map_err(anyhow::Error::from_boxed)
}
