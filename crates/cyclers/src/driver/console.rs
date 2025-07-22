use futures_concurrency::stream::Zip as _;
use futures_core::Stream;
use futures_lite::StreamExt as _;
use futures_rx::stream_ext::share::Shared;
use futures_rx::{PublishSubject, RxExt as _};
use tokio_util::codec::{FramedRead, LinesCodec, LinesCodecError};

use super::{Driver, Source};
use crate::BoxError;

pub struct ConsoleDriver;

pub struct ConsoleSource<Sink>
where
    Sink: Stream,
{
    sink: Shared<Sink, PublishSubject<Sink::Item>>,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ConsoleCommand {
    Print(String),
    Read,
}

impl<Sink> Driver<Sink> for ConsoleDriver
where
    Sink: Stream<Item = ConsoleCommand>,
{
    type Input = ConsoleCommand;
    type Source = ConsoleSource<Sink>;

    fn call(self, sink: Sink) -> (Self::Source, impl Future<Output = Result<(), BoxError>>) {
        let sink = sink.share();
        let print = sink
            .clone()
            .filter(|command| matches!(**command, ConsoleCommand::Print(_)));

        (ConsoleSource { sink }, async move {
            print
                .for_each(|command| match &*command {
                    ConsoleCommand::Print(s) => println!("{s}"),
                    _ => unreachable!(),
                })
                .await;
            Ok(())
        })
    }
}

impl<Sink> Source for ConsoleSource<Sink> where Sink: Stream {}

impl<Sink> ConsoleSource<Sink>
where
    Sink: Stream<Item = ConsoleCommand>,
{
    pub fn read(&self) -> impl Stream<Item = Result<String, LinesCodecError>> + use<Sink> {
        let read = self
            .sink
            .clone()
            .filter(|command| **command == ConsoleCommand::Read);
        let lines = FramedRead::new(tokio::io::stdin(), LinesCodec::new());

        (read, lines).zip().map(|(_command, line)| line)
    }
}
