//! Run with:
//!
//! ```shell
//! cargo run --bin http-wiki
//! ```

use std::io;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use cyclers::{ArcError, BoxError};
use cyclers_http::{HttpCommand, HttpDriver, HttpSource, Request};
use cyclers_terminal::{TerminalCommand, TerminalDriver, TerminalSource};
use futures_concurrency::stream::{Chain as _, Zip as _};
use futures_lite::{StreamExt as _, stream};
use futures_rx::RxExt as _;
#[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
use tokio::main;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use url::Url;
#[cfg(all(target_os = "wasi", target_env = "p2"))]
use wstd::main;

#[main]
async fn main() -> Result<ExitCode> {
    init_tracing_subscriber();

    cyclers::run(
        |_terminal_source: TerminalSource<_>, http_source: HttpSource<_>| {
            // Set up the URL for querying the MediaWiki API.
            let url = stream::once(
                Url::parse_with_params("https://en.wikipedia.org/w/api.php", &[
                    ("action", "query"),
                    ("generator", "random"),
                    ("grnnamespace", "0"),
                    ("grnminsize", "500"),
                    ("grnlimit", "1"),
                    ("prop", "info|extracts"),
                    ("inprop", "url"),
                    ("exchars", "1200"),
                    ("explaintext", "true"),
                    ("format", "json"),
                ])
                .context("failed to parse url"),
            );

            // Prepare the HTTP request to send to the server.
            let send_request = url.map(|url| {
                url.and_then(|url| {
                    Ok(HttpCommand::SendRequest({
                        Request::builder()
                            .method("GET")
                            .uri(url.as_str())
                            .body(vec![].into())
                            .context("failed to build request")?
                    }))
                })
            });

            // Receive the HTTP response from the server.
            let response = http_source.response().map(|res| {
                res.map_err(anyhow::Error::from_boxed)
                    .context("failed to process response")
            });

            // Parse response body as JSON.
            let response_json = response
                .map(|res| {
                    res.and_then(|res| {
                        let json: serde_json::Value = serde_json::from_slice(res.body())
                            .context("failed to parse response body as JSON")?;
                        Ok(json)
                    })
                    .map_err(|err| ArcError::from(BoxError::from(err)))
                })
                .share();

            // Get the first "page" from the query result.
            let page = response_json
                .clone()
                .map(|json| match &*json {
                    Ok(json) => match &json.pointer("/query/pages") {
                        Some(serde_json::Value::Object(pages)) => match pages.iter().next() {
                            Some((_, serde_json::Value::Object(page))) => Ok(page.clone()),
                            _ => Err(anyhow!(
                                "\"query\".\"pages\".{{id}} not found or not an object"
                            )),
                        },
                        _ => Err(anyhow!("\"query\".\"pages\" not found or not an object")),
                    }
                    .map_err(|err| ArcError::from(BoxError::from(err))),
                    Err(err) => Err(Arc::clone(err)),
                })
                .share();

            let print_title = page.clone().map(|page| match &*page {
                Ok(page) => match &page["title"] {
                    serde_json::Value::String(s) => Ok(TerminalCommand::Write(format!("{s}\n"))),
                    _ => Err(anyhow!("\"title\" not found or not a string")),
                }
                .map_err(|err| ArcError::from(BoxError::from(err))),
                Err(err) => Err(Arc::clone(err)),
            });

            let print_extract = page.clone().map(|page| match &*page {
                Ok(page) => match &page["extract"] {
                    serde_json::Value::String(s) => Ok(TerminalCommand::Write(format!("{s}\n"))),
                    _ => Err(anyhow!("\"extract\" not found or not a string")),
                }
                .map_err(|err| ArcError::from(BoxError::from(err))),
                Err(err) => Err(Arc::clone(err)),
            });

            let print_url = page.clone().map(|page| match &*page {
                Ok(page) => match &page["canonicalurl"] {
                    serde_json::Value::String(s) => Ok(TerminalCommand::Write(format!("{s}\n"))),
                    _ => Err(anyhow!("\"canonicalurl\" not found or not a string")),
                }
                .map_err(|err| ArcError::from(BoxError::from(err))),
                Err(err) => Err(Arc::clone(err)),
            });

            let http_sink = (send_request,).chain();

            let terminal_sink = (
                // Print out the info with separators.
                (print_title, print_extract, print_url)
                    .zip()
                    .flat_map(|(title, extract, url)| {
                        stream::iter([
                            title,
                            Ok(TerminalCommand::Write(format!("{:─<80}\n", ""))),
                            extract,
                            Ok(TerminalCommand::Write(format!("{:─<80}\n", ""))),
                            url,
                        ])
                    }),
            )
                .chain();

            (terminal_sink, http_sink)
        },
        (TerminalDriver, HttpDriver),
    )
    .await
    .map_err(anyhow::Error::from_boxed)
}

fn init_tracing_subscriber() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                #[cfg(not(debug_assertions))]
                {
                    "info".into()
                }
                #[cfg(debug_assertions)]
                {
                    format!(
                        "{crate}=debug,cyclers=debug,cyclers_http=debug,cyclers_terminal=debug",
                        crate = env!("CARGO_CRATE_NAME"),
                    )
                    .into()
                }
            }),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(io::stderr))
        .init();
}
