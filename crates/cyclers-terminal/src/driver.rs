use std::error::Error;
#[cfg(all(unix, not(target_family = "wasm"), not(target_os = "macos")))]
use std::io::IsTerminal as _;
use std::process::ExitCode;
use std::{fmt, io};

#[cfg(all(target_os = "wasi", target_env = "p2"))]
use bytes::BytesMut;
use cyclers::BoxError;
use cyclers::driver::{Driver, Source};
use futures_concurrency::stream::Zip as _;
use futures_lite::{Stream, StreamExt as _, stream};
use futures_rx::stream_ext::share::Shared;
use futures_rx::{PublishSubject, RxExt as _};
#[cfg(not(target_family = "wasm"))]
use tokio::io::{AsyncRead, AsyncWriteExt as _};
#[cfg(not(target_family = "wasm"))]
use tokio_util::codec::{FramedRead, LinesCodec, LinesCodecError};
#[cfg(all(feature = "tracing", target_os = "wasi", target_env = "p2"))]
use tracing::trace;
#[cfg(feature = "tracing")]
use tracing::{debug, instrument};
#[cfg(feature = "tracing")]
use tracing_futures::Instrument as _;
#[cfg(all(target_os = "wasi", target_env = "p2"))]
use wstd::io::{AsyncRead as _, AsyncWrite as _};

#[cfg(all(unix, not(target_family = "wasm"), not(target_os = "macos")))]
use crate::sys::unix::AsyncStdin;

pub struct TerminalDriver;

pub struct TerminalSource<Sink>
where
    Sink: Stream,
{
    sink: Shared<Sink, PublishSubject<Sink::Item>>,
}

#[derive(Clone, PartialEq, Debug)]
#[non_exhaustive]
pub enum TerminalCommand {
    /// Prints to the standard output. A newline is not printed at the end of
    /// the message.
    ///
    /// Note that stdout is frequently line-buffered by default so it may be
    /// necessary to use [`TerminalCommand::Flush`] to ensure the output is
    /// emitted immediately.
    Write(String),
    /// Reads a line of input from the standard input.
    ReadLine,
    /// Flushes the standard output stream, ensuring that all intermediately
    /// buffered contents reach their destination.
    Flush,
    /// Exits with the specified status code.
    Exit(ExitCode),
}

/// An error returned from [`TerminalSource::read_line`].
#[derive(Debug)]
#[non_exhaustive]
pub struct ReadLineError {
    kind: ReadLineErrorKind,
    inner: BoxError,
}

/// The various types of errors that can cause [`TerminalSource::read_line`] to
/// fail.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReadLineErrorKind {
    /// Failed to open stdin.
    #[cfg(not(target_family = "wasm"))]
    Stdin,
    /// Failed to process line.
    #[cfg(not(target_family = "wasm"))]
    Line,
    /// Failed to read bytes.
    #[cfg(all(target_os = "wasi", target_env = "p2"))]
    Read,
    /// Invalid UTF-8 encountered.
    #[cfg(all(target_os = "wasi", target_env = "p2"))]
    InvalidUtf8,
}

impl<Sink> Driver<Sink> for TerminalDriver
where
    Sink: Stream<Item = TerminalCommand>,
{
    type Source = TerminalSource<Sink>;
    type Termination = ExitCode;

    fn call(
        self,
        sink: Sink,
    ) -> (
        Self::Source,
        impl Future<Output = Result<Self::Termination, BoxError>>,
    ) {
        let sink = sink.share();

        (TerminalSource { sink: sink.clone() }, self.run(sink))
    }
}

impl TerminalDriver {
    #[cfg_attr(feature = "tracing", instrument(level = "debug", skip(self, sink)))]
    async fn run<Sink>(
        self,
        sink: Shared<Sink, PublishSubject<Sink::Item>>,
    ) -> Result<<Self as Driver<Sink>>::Termination, BoxError>
    where
        Sink: Stream<Item = TerminalCommand>,
    {
        let mut sink = sink.filter(|command| {
            matches!(
                **command,
                TerminalCommand::Write(..) | TerminalCommand::Flush | TerminalCommand::Exit(..)
            )
        });

        let mut stdout = {
            #[cfg(not(target_family = "wasm"))]
            {
                tokio::io::stdout()
            }
            #[cfg(all(target_os = "wasi", target_env = "p2"))]
            {
                wstd::io::stdout()
            }
        };

        while let Some(command) = sink.next().await {
            match &*command {
                TerminalCommand::Write(out) => {
                    #[cfg(feature = "tracing")]
                    debug!(out, "writing to stdout");

                    #[cfg(any(
                        not(target_family = "wasm"),
                        all(target_os = "wasi", target_env = "p2")
                    ))]
                    {
                        stdout.write_all(out.as_bytes()).await?;
                    }
                },
                TerminalCommand::Flush => {
                    #[cfg(feature = "tracing")]
                    debug!("flushing stdout");

                    #[cfg(any(
                        not(target_family = "wasm"),
                        all(target_os = "wasi", target_env = "p2")
                    ))]
                    {
                        stdout.flush().await?;
                    }
                },
                TerminalCommand::Exit(exit_code) => {
                    #[cfg(feature = "tracing")]
                    debug!(?exit_code, "exiting");

                    return Ok(*exit_code);
                },
                _ => unreachable!(),
            }
        }

        Ok(ExitCode::SUCCESS)
    }
}

impl<Sink> Source for TerminalSource<Sink> where Sink: Stream {}

#[cfg(not(target_family = "wasm"))]
impl<Sink> TerminalSource<Sink>
where
    Sink: Stream<Item = TerminalCommand>,
{
    #[cfg_attr(feature = "tracing", instrument(level = "debug", skip(self)))]
    pub fn read_line(&self) -> impl Stream<Item = Result<String, ReadLineError>> + use<Sink> {
        let read_line = self
            .sink
            .clone()
            .filter(|command| matches!(**command, TerminalCommand::ReadLine));

        let stdin: io::Result<Box<dyn AsyncRead + Send>> = {
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                if std::io::stdin().is_terminal() {
                    #[cfg(feature = "tracing")]
                    debug!("using AsyncStdin as stdin is a terminal");

                    match AsyncStdin::new() {
                        Ok(stdin) => Ok(Box::new(stdin)),
                        Err(err) => Err(err),
                    }
                } else {
                    #[cfg(feature = "tracing")]
                    debug!("using tokio::io::Stdin as fallback");

                    Ok(Box::new(tokio::io::stdin()))
                }
            }
            #[cfg(any(not(unix), target_os = "macos"))]
            {
                #[cfg(feature = "tracing")]
                debug!("using tokio::io::Stdin as fallback");

                Ok(Box::new(tokio::io::stdin()))
            }
        };

        let line = async move {
            let stdin = stdin.map_err(|err| ReadLineError {
                kind: ReadLineErrorKind::Stdin,
                inner: err.into(),
            })?;
            let stdin = Box::into_pin(stdin);

            Ok(FramedRead::new(stdin, LinesCodec::new()))
        };
        let line = stream::once_future(line)
            .map(|line| match line {
                Ok(line) => line
                    .map(|line| {
                        line.map_err(|err| ReadLineError {
                            kind: ReadLineErrorKind::Line,
                            inner: err.into(),
                        })
                    })
                    .boxed(),
                Err(err) => stream::once(Err(err)).boxed(),
            })
            .flatten();

        #[cfg(feature = "tracing")]
        let read_line = read_line
            .inspect(|_read_line| debug!("reading line from stdin"))
            .in_current_span();
        #[cfg(feature = "tracing")]
        let line = line
            .inspect(|line| {
                if let Ok(line) = line {
                    debug!(line, "read line from stdin");
                }
            })
            .in_current_span();

        (read_line, line).zip().map(|(_read_line, line)| line)
    }
}

#[cfg(all(target_os = "wasi", target_env = "p2"))]
impl<Sink> TerminalSource<Sink>
where
    Sink: Stream<Item = TerminalCommand>,
{
    #[cfg_attr(feature = "tracing", instrument(level = "debug", skip(self)))]
    pub fn read_line(&self) -> impl Stream<Item = Result<String, ReadLineError>> + use<Sink> {
        let read_line = self
            .sink
            .clone()
            .filter(|command| matches!(**command, TerminalCommand::ReadLine));

        let stdin = wstd::io::stdin();

        const INITIAL_CAPACITY: usize = 8 * 1024;
        const CHUNK_SIZE: usize = 1024;

        let buf = BytesMut::with_capacity(INITIAL_CAPACITY);
        let eof = false;
        let line = stream::unfold(
            (stdin, buf, eof),
            |(mut stdin, mut buf, mut eof)| async move {
                loop {
                    if let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                        #[cfg(feature = "tracing")]
                        trace!(pos, "found newline");

                        let mut line: Vec<u8> = buf.split_to(pos.checked_add(1).unwrap()).into();
                        let _newline = line.pop().unwrap();
                        let line = String::from_utf8(line).map_err(|err| ReadLineError {
                            kind: ReadLineErrorKind::InvalidUtf8,
                            inner: err.into(),
                        });

                        return Some((line, (stdin, buf, eof)));
                    }

                    if eof {
                        #[cfg(feature = "tracing")]
                        debug!("reached EOF before a newline is found");

                        return None;
                    }

                    let prev_len = buf.len();
                    buf.resize(
                        prev_len
                            .checked_add(CHUNK_SIZE)
                            .expect("`new_len` should not overflow `usize`"),
                        0u8,
                    );

                    match stdin.read(&mut buf[prev_len..]).await {
                        Ok(bytes_read) => {
                            #[cfg(feature = "tracing")]
                            trace!(bytes_read, "read from stdin");

                            if bytes_read == 0 {
                                #[cfg(feature = "tracing")]
                                trace!("reached EOF");

                                eof = true;
                            }
                            buf.truncate(
                                prev_len
                                    .checked_add(bytes_read)
                                    .expect("`len` should not overflow `usize`"),
                            );
                            continue;
                        },
                        Err(err) => {
                            return Some((
                                Err(ReadLineError {
                                    kind: ReadLineErrorKind::Read,
                                    inner: err.into(),
                                }),
                                (stdin, buf, eof),
                            ));
                        },
                    }
                }
            },
        );

        #[cfg(feature = "tracing")]
        let read_line = read_line
            .inspect(|_read_line| debug!("reading line from stdin"))
            .in_current_span();
        #[cfg(feature = "tracing")]
        let line = line
            .inspect(|line| {
                if let Ok(line) = line {
                    debug!(line, "read line from stdin");
                }
            })
            .in_current_span();

        (read_line, line).zip().map(|(_read_line, line)| line)
    }
}

impl fmt::Display for ReadLineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            #[cfg(not(target_family = "wasm"))]
            ReadLineErrorKind::Stdin => {
                let err = self.inner.downcast_ref::<io::Error>().unwrap();
                write!(f, "failed to open stdin: {err}")
            },
            #[cfg(not(target_family = "wasm"))]
            ReadLineErrorKind::Line => {
                let err = self.inner.downcast_ref::<LinesCodecError>().unwrap();
                write!(f, "failed to process line: {err}")
            },
            #[cfg(all(target_os = "wasi", target_env = "p2"))]
            ReadLineErrorKind::Read => {
                let err = self.inner.downcast_ref::<io::Error>().unwrap();
                write!(f, "failed to read bytes: {err}")
            },
            #[cfg(all(target_os = "wasi", target_env = "p2"))]
            ReadLineErrorKind::InvalidUtf8 => {
                let err = self
                    .inner
                    .downcast_ref::<std::string::FromUtf8Error>()
                    .unwrap();
                write!(f, "invalid UTF-8 encountered: {err}")
            },
        }
    }
}

impl Error for ReadLineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.kind {
            #[cfg(not(target_family = "wasm"))]
            ReadLineErrorKind::Stdin => {
                let err = self.inner.downcast_ref::<io::Error>().unwrap();
                Some(err)
            },
            #[cfg(not(target_family = "wasm"))]
            ReadLineErrorKind::Line => {
                let err = self.inner.downcast_ref::<LinesCodecError>().unwrap();
                Some(err)
            },
            #[cfg(all(target_os = "wasi", target_env = "p2"))]
            ReadLineErrorKind::Read => {
                let err = self.inner.downcast_ref::<io::Error>().unwrap();
                Some(err)
            },
            #[cfg(all(target_os = "wasi", target_env = "p2"))]
            ReadLineErrorKind::InvalidUtf8 => {
                let err = self
                    .inner
                    .downcast_ref::<std::string::FromUtf8Error>()
                    .unwrap();
                Some(err)
            },
        }
    }
}

impl ReadLineError {
    /// Returns the corresponding [`ReadLineErrorKind`] for this error.
    #[must_use]
    pub const fn kind(&self) -> &ReadLineErrorKind {
        &self.kind
    }
}
