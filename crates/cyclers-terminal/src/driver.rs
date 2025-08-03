use std::error::Error;
#[cfg(all(
    unix,
    not(any(target_family = "wasm", target_os = "wasi")),
    not(target_os = "macos")
))]
use std::io::IsTerminal as _;
use std::process::ExitCode;
use std::{fmt, io};

#[cfg(all(target_os = "wasi", target_env = "p2"))]
use bytes::BytesMut;
use cyclers::BoxError;
use cyclers::driver::{Driver, Source};
use futures_concurrency::stream::{Merge as _, Zip as _};
use futures_lite::{Stream, StreamExt as _, pin, stream};
use futures_rx::stream_ext::share::Shared;
use futures_rx::{PublishSubject, RxExt as _};
#[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
use tokio::io::{AsyncRead, AsyncWriteExt as _};
#[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
use tokio_util::codec::{FramedRead, LinesCodec, LinesCodecError};
#[cfg(all(target_os = "wasi", target_env = "p2"))]
use wstd::io::{AsyncRead as _, AsyncWrite as _};

#[cfg(all(
    unix,
    not(any(target_family = "wasm", target_os = "wasi")),
    not(target_os = "macos")
))]
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
pub struct TerminalReadError {
    kind: TerminalReadErrorKind,
    inner: BoxError,
}

/// The various types of errors that can cause [`TerminalSource::read_line`] to
/// fail.
#[derive(Debug)]
#[non_exhaustive]
pub enum TerminalReadErrorKind {
    /// Failed to open stdin.
    Stdin,
    /// Failed to process line.
    Line,
    /// Failed to read bytes.
    Read,
    /// Invalid UTF-8 encountered.
    InvalidUtf8,
}

impl<Sink> Driver<Sink> for TerminalDriver
where
    Sink: Stream<Item = TerminalCommand>,
{
    type Input = TerminalCommand;
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
        let write = sink
            .clone()
            .filter(|command| matches!(**command, TerminalCommand::Write(..)));
        let flush = sink
            .clone()
            .filter(|command| matches!(**command, TerminalCommand::Flush));
        let exit = sink
            .clone()
            .filter(|command| matches!(**command, TerminalCommand::Exit(..)));

        let mut stdout = {
            #[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
            {
                tokio::io::stdout()
            }
            #[cfg(all(target_os = "wasi", target_env = "p2"))]
            {
                wstd::io::stdout()
            }
        };

        (TerminalSource { sink }, async move {
            let s = (write, flush, exit).merge();
            pin!(s);

            while let Some(command) = s.next().await {
                match &*command {
                    TerminalCommand::Write(s) => {
                        #[cfg(any(
                            not(any(target_family = "wasm", target_os = "wasi")),
                            all(target_os = "wasi", target_env = "p2")
                        ))]
                        {
                            stdout.write_all(s.as_bytes()).await?;
                        }
                        #[cfg(all(
                            any(target_family = "wasm", target_os = "wasi"),
                            not(all(target_os = "wasi", target_env = "p2"))
                        ))]
                        {
                            unimplemented!();
                        }
                    },
                    TerminalCommand::Flush => {
                        #[cfg(any(
                            not(any(target_family = "wasm", target_os = "wasi")),
                            all(target_os = "wasi", target_env = "p2")
                        ))]
                        {
                            stdout.flush().await?;
                        }
                        #[cfg(all(
                            any(target_family = "wasm", target_os = "wasi"),
                            not(all(target_os = "wasi", target_env = "p2"))
                        ))]
                        {
                            unimplemented!();
                        }
                    },
                    TerminalCommand::Exit(exit_code) => return Ok(*exit_code),
                    _ => unreachable!(),
                }
            }

            Ok(ExitCode::SUCCESS)
        })
    }
}

impl<Sink> Source for TerminalSource<Sink> where Sink: Stream {}

#[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
impl<Sink> TerminalSource<Sink>
where
    Sink: Stream<Item = TerminalCommand>,
{
    pub fn read_line(&self) -> impl Stream<Item = Result<String, TerminalReadError>> + use<Sink> {
        let read_line = self
            .sink
            .clone()
            .filter(|command| matches!(**command, TerminalCommand::ReadLine));

        let stdin: io::Result<Box<dyn AsyncRead + Send>> = {
            #[cfg(all(unix, not(target_os = "macos")))]
            {
                if std::io::stdin().is_terminal() {
                    match AsyncStdin::new() {
                        Ok(stdin) => Ok(Box::new(stdin)),
                        Err(err) => Err(err),
                    }
                } else {
                    Ok(Box::new(tokio::io::stdin()))
                }
            }
            #[cfg(any(not(unix), target_os = "macos"))]
            {
                Ok(Box::new(tokio::io::stdin()))
            }
        };

        let line = async move {
            let stdin = stdin.map_err(|err| TerminalReadError {
                kind: TerminalReadErrorKind::Stdin,
                inner: err.into(),
            })?;
            let stdin = Box::into_pin(stdin);

            Ok(FramedRead::new(stdin, LinesCodec::new()))
        };
        let line = stream::once_future(line)
            .map(|line| match line {
                Ok(line) => line
                    .map(|line| {
                        line.map_err(|err| TerminalReadError {
                            kind: TerminalReadErrorKind::Line,
                            inner: err.into(),
                        })
                    })
                    .boxed(),
                Err(err) => stream::once(Err(err)).boxed(),
            })
            .flatten();

        (read_line, line).zip().map(|(_read_line, line)| line)
    }
}

#[cfg(all(target_os = "wasi", target_env = "p2"))]
impl<Sink> TerminalSource<Sink>
where
    Sink: Stream<Item = TerminalCommand>,
{
    pub fn read_line(&self) -> impl Stream<Item = Result<String, TerminalReadError>> + use<Sink> {
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
                        let mut line: Vec<u8> = buf.split_to(pos.checked_add(1).unwrap()).into();
                        let _newline = line.pop().unwrap();
                        let line = String::from_utf8(line).map_err(|err| TerminalReadError {
                            kind: TerminalReadErrorKind::InvalidUtf8,
                            inner: err.into(),
                        });

                        return Some((line, (stdin, buf, eof)));
                    }

                    if eof {
                        return None;
                    }

                    let prev_len = buf.len();
                    buf.resize(prev_len + CHUNK_SIZE, 0u8);

                    match stdin.read(&mut buf[prev_len..]).await {
                        Ok(bytes_read) => {
                            if bytes_read == 0 {
                                eof = true;
                            }
                            buf.truncate(prev_len + bytes_read);
                            continue;
                        },
                        Err(err) => {
                            return Some((
                                Err(TerminalReadError {
                                    kind: TerminalReadErrorKind::Read,
                                    inner: err.into(),
                                }),
                                (stdin, buf, eof),
                            ));
                        },
                    }
                }
            },
        );

        (read_line, line).zip().map(|(_read_line, line)| line)
    }
}

impl fmt::Display for TerminalReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            TerminalReadErrorKind::Stdin => {
                let err = self.inner.downcast_ref::<io::Error>().unwrap();
                write!(f, "failed to open stdin: {err}")
            },
            TerminalReadErrorKind::Line => {
                #[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
                {
                    let err = self.inner.downcast_ref::<LinesCodecError>().unwrap();
                    write!(f, "failed to process line: {err}")
                }
                #[cfg(any(target_family = "wasm", target_os = "wasi"))]
                {
                    unimplemented!();
                }
            },
            TerminalReadErrorKind::Read => {
                let err = self.inner.downcast_ref::<io::Error>().unwrap();
                write!(f, "failed to read bytes: {err}")
            },
            TerminalReadErrorKind::InvalidUtf8 => {
                let err = self
                    .inner
                    .downcast_ref::<std::string::FromUtf8Error>()
                    .unwrap();
                write!(f, "invalid UTF-8 encountered: {err}")
            },
        }
    }
}

impl Error for TerminalReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.kind {
            TerminalReadErrorKind::Stdin => {
                let err = self.inner.downcast_ref::<io::Error>().unwrap();
                Some(err)
            },
            TerminalReadErrorKind::Line => {
                #[cfg(not(any(target_family = "wasm", target_os = "wasi")))]
                {
                    let err = self.inner.downcast_ref::<LinesCodecError>().unwrap();
                    Some(err)
                }
                #[cfg(any(target_family = "wasm", target_os = "wasi"))]
                {
                    unimplemented!();
                }
            },
            TerminalReadErrorKind::Read => {
                let err = self.inner.downcast_ref::<io::Error>().unwrap();
                Some(err)
            },
            TerminalReadErrorKind::InvalidUtf8 => {
                let err = self
                    .inner
                    .downcast_ref::<std::string::FromUtf8Error>()
                    .unwrap();
                Some(err)
            },
        }
    }
}
