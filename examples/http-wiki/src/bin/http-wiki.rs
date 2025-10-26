//! Run with:
//!
//! ```shell
//! cargo run --bin http-wiki
//! ```

use std::io;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow, bail};
use cyclers::{ArcError, BoxError};
use cyclers_http::{HttpCommand, HttpDriver, HttpSource, Request};
use cyclers_terminal::{TerminalCommand, TerminalDriver, TerminalSource};
use futures_concurrency::stream::{Chain as _, Zip as _};
use futures_lite::{StreamExt as _, stream};
use futures_rx::RxExt as _;
use http::header;
#[cfg(not(target_family = "wasm"))]
use tokio::main;
use tracing::debug;
#[cfg(target_family = "wasm")]
use tracing_subscriber::layer::Layer;
#[cfg(not(target_family = "wasm"))]
use tracing_subscriber::layer::Layer as _;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use url::Url;
#[cfg(all(target_os = "wasi", target_env = "p2"))]
use wstd::main;

const USER_AGENT: &str =
    "example-http-wiki/0.1.0 (https://github.com/teohhanhui/cyclers) cyclers-http/0.1.0";

#[main]
async fn main() -> Result<ExitCode> {
    init_tracing_subscriber();

    cyclers::run(
        |terminal_source: TerminalSource<_>, http_source: HttpSource<_>| {
            // Receive lines of input text that have been read from the terminal.
            let input = terminal_source
                .lines()
                .map(|input| {
                    input
                        .context("failed to read line from terminal")
                        .map_err(|err| ArcError::from(BoxError::from(err)))
                })
                .share();

            // Set up the URL for querying the MediaWiki API.
            let url = input.clone().filter_map(|input| match &*input {
                Ok(input) => Some(
                    Url::parse_with_params("https://en.wikipedia.org/w/api.php", &[
                        ("action", "query"),
                        ("generator", "search"),
                        ("gsrsearch", input),
                        ("gsrnamespace", "0"),
                        ("gsrlimit", "1"),
                        ("prop", "info|extracts"),
                        ("inprop", "url"),
                        ("exchars", "1200"),
                        ("explaintext", "true"),
                        ("format", "json"),
                    ])
                    .context("failed to parse url"),
                ),
                Err(_) => None,
            });

            // Prepare the HTTP request to send to the server.
            let send_request = url.map(|url| {
                url.and_then(|url| {
                    Ok(HttpCommand::from({
                        let mut req = Request::builder().method("GET").uri(url.as_str());

                        let headers = req.headers_mut().unwrap();
                        headers.insert(header::USER_AGENT, USER_AGENT.parse().unwrap());

                        req.body("".into()).context("failed to build request")?
                    }))
                })
            });

            // Receive the HTTP response from the server.
            let responses = http_source.responses().map(|res| {
                res.map_err(anyhow::Error::from_boxed)
                    .context("failed to process response")
            });

            // Parse response body as JSON.
            let json_responses = responses
                .map(|res| {
                    res.and_then(|res| {
                        if !res.status().is_success() {
                            debug!(?res, "received unsuccessful response");
                            if res.status().is_client_error() {
                                bail!("client error");
                            } else if res.status().is_server_error() {
                                bail!("server error");
                            } else {
                                bail!("unsuccessful response");
                            }
                        }

                        let json: serde_json::Value = serde_json::from_slice(res.body())
                            .context("failed to parse response body as JSON")?;

                        Ok(json)
                    })
                    .map_err(|err| ArcError::from(BoxError::from(err)))
                })
                .share();

            // Get the first "page" from the query result.
            let page = json_responses
                .clone()
                .filter_map(|json| match &*json {
                    Ok(json) => Some({
                        // Don't error if there are no results.
                        json.pointer("/query")?;

                        match &json.pointer("/query/pages") {
                            Some(serde_json::Value::Object(pages)) => match pages.iter().next() {
                                Some((_, serde_json::Value::Object(page))) => Ok(page.clone()),
                                _ => Err(anyhow!(
                                    "\"query\".\"pages\".{{id}} not found or not an object"
                                )),
                            },
                            _ => Err(anyhow!("\"query\".\"pages\" not found or not an object")),
                        }
                        .map_err(|err| ArcError::from(BoxError::from(err)))
                    }),
                    Err(err) => Some(Err(Arc::clone(err))),
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

            // After reading a line from the terminal, we will print the prompt again.
            let print_prompt = input.clone().map(|input| match &*input {
                Ok(_input) => stream::iter(vec![
                    Ok(TerminalCommand::Write("".to_owned())),
                    Ok(TerminalCommand::Flush),
                ]),
                Err(err) => stream::iter(vec![Err(Arc::clone(err))]),
            });

            let first_read = stream::iter([
                TerminalCommand::Write("Search: ".to_owned()),
                TerminalCommand::Flush,
                TerminalCommand::ReadLine,
            ]);

            let read = stream::repeat(stream::iter([
                TerminalCommand::Write("Search: ".to_owned()),
                TerminalCommand::Flush,
                TerminalCommand::ReadLine,
            ]));

            // Print out the info with separators.
            let print_article =
                (print_title, print_extract, print_url)
                    .zip()
                    .map(|(title, extract, url)| {
                        stream::iter(vec![
                            Ok(TerminalCommand::Write(format!("{:─<80}\n", ""))),
                            title,
                            Ok(TerminalCommand::Write(format!("{:─<80}\n", ""))),
                            extract,
                            Ok(TerminalCommand::Write(format!("{:─<80}\n", ""))),
                            url,
                            Ok(TerminalCommand::Write(format!("{:─<80}\n", ""))),
                        ])
                    });

            let no_results = json_responses
                .clone()
                .filter_map(|json| {
                    let json = json.as_ref().ok()?;
                    match json.pointer("/query") {
                        None => Some(TerminalCommand::Write("No results\n".to_owned())),
                        _ => None,
                    }
                })
                .map(|no_results| stream::iter(vec![Ok(no_results)]));

            let terminal_sink = {
                // Print prompt, read search term, print article, and repeat...
                (
                    first_read.map(Ok),
                    (print_article.or(no_results), print_prompt, read)
                        .zip()
                        .flat_map(|(print_article, print_prompt, read)| {
                            (print_article, print_prompt, read.map(Ok)).chain()
                        }),
                )
                    .chain()
            };

            (terminal_sink, http_sink)
        },
        (TerminalDriver, HttpDriver),
    )
    .await
    .map_err(anyhow::Error::from_boxed)
}

fn init_tracing_subscriber() {
    tracing_subscriber::registry()
        .with({
            #[cfg(not(target_family = "wasm"))]
            {
                Some(console_subscriber::spawn().boxed())
            }
            #[cfg(target_family = "wasm")]
            {
                None::<Box<dyn Layer<_> + Send + Sync + 'static>>
            }
        })
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(io::stderr)
                .with_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                        #[cfg(not(debug_assertions))]
                        {
                            "info".into()
                        }
                        #[cfg(debug_assertions)]
                        {
                            format!(
                                "{crate}=debug,cyclers=debug,cyclers_http=debug,\
                                 cyclers_terminal=debug,info",
                                crate = env!("CARGO_CRATE_NAME"),
                            )
                            .into()
                        }
                    }),
                ),
        )
        .init();
}
