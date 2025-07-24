use std::fs::File;
use std::io::{self, Read as _};
use std::os::unix::fs::OpenOptionsExt as _;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use rustix::fs::OFlags;
use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, ReadBuf};

pub(crate) struct AsyncStdin {
    inner: AsyncFd<File>,
}

impl AsyncRead for AsyncStdin {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            let mut guard = ready!(self.inner.poll_read_ready(cx))?;

            let unfilled = buf.initialize_unfilled();
            match guard.try_io(|inner| inner.get_ref().read(unfilled)) {
                Ok(Ok(len)) => {
                    buf.advance(len);
                    return Poll::Ready(Ok(()));
                },
                Ok(Err(err)) => return Poll::Ready(Err(err)),
                Err(_would_block) => continue,
            }
        }
    }
}

impl AsyncStdin {
    pub(crate) fn new() -> io::Result<Self> {
        let tty = tty_non_blocking()?;
        Ok(Self {
            inner: AsyncFd::new(tty)?,
        })
    }
}

pub(crate) fn tty_non_blocking() -> io::Result<File> {
    File::options()
        .read(true)
        .write(true)
        .custom_flags(OFlags::NONBLOCK.bits().try_into().unwrap())
        .open("/dev/tty")
}
