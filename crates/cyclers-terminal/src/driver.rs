use std::error::Error;
#[cfg(unix)]
use std::io::IsTerminal as _;
use std::{fmt, io};

use cyclers::BoxError;
use cyclers::driver::{Driver, Source};
use futures_concurrency::stream::{Merge as _, Zip as _};
use futures_lite::{Stream, StreamExt as _, pin, stream};
use futures_rx::stream_ext::share::Shared;
use futures_rx::{PublishSubject, RxExt as _};
use tokio::io::{AsyncRead, AsyncWriteExt as _};
use tokio_util::codec::{FramedRead, LinesCodec, LinesCodecError};

#[cfg(unix)]
use crate::sys::unix::AsyncStdin;

pub struct TerminalDriver;

pub struct TerminalSource<Sink>
where
    Sink: Stream,
{
    sink: Shared<Sink, PublishSubject<Sink::Item>>,
}

#[derive(Clone, Eq, PartialEq, Debug)]
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
}

impl<Sink> Driver<Sink> for TerminalDriver
where
    Sink: Stream<Item = TerminalCommand>,
{
    type Input = TerminalCommand;
    type Source = TerminalSource<Sink>;

    fn call(self, sink: Sink) -> (Self::Source, impl Future<Output = Result<(), BoxError>>) {
        let sink = sink.share();
        let write = sink
            .clone()
            .filter(|command| matches!(**command, TerminalCommand::Write(_)));
        let flush = sink
            .clone()
            .filter(|command| matches!(**command, TerminalCommand::Flush));

        let mut stdout = tokio::io::stdout();

        (TerminalSource { sink }, async move {
            let s = (write, flush).merge();
            pin!(s);

            while let Some(command) = s.next().await {
                match &*command {
                    TerminalCommand::Write(s) => {
                        stdout.write_all(s.as_bytes()).await?;
                    },
                    TerminalCommand::Flush => {
                        stdout.flush().await?;
                    },
                    _ => unreachable!(),
                }
            }
            Ok(())
        })
    }
}

impl<Sink> Source for TerminalSource<Sink> where Sink: Stream {}

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

impl fmt::Display for TerminalReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            TerminalReadErrorKind::Stdin => {
                let err = self.inner.downcast_ref::<io::Error>().unwrap();
                write!(f, "failed to open stdin: {err}")
            },
            TerminalReadErrorKind::Line => {
                let err = self.inner.downcast_ref::<LinesCodecError>().unwrap();
                write!(f, "failed to process line: {err}")
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
                let err = self.inner.downcast_ref::<LinesCodecError>().unwrap();
                Some(err)
            },
        }
    }
}
