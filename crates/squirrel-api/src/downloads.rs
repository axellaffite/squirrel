//! Download concurrency: a slot is held for as long as a response body streams.
//!
//! There is no write-idle timeout: a client that stops reading keeps its slot
//! until the TCP connection fails (kernel keepalive / retransmission limits) or
//! the server shuts down. The per-user cap bounds what one account can pin, and
//! visitors of public links have slots of their own ([`DownloadLimits::public`]):
//! whatever they pin, the users' downloads are not held up.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, ReadBuf};

use crate::upload_runtime::{Slot, Slots};

/// Download limits (part of [`crate::ApiConfig`]).
#[derive(Debug, Clone)]
pub struct DownloadLimits {
    /// Response bodies streaming at once, all users together.
    pub concurrency: usize,
    /// Response bodies one user may stream at once.
    pub per_user: usize,
    /// How long a request waits for a global slot before answering 503 `busy`.
    pub wait: Duration,
}

impl Default for DownloadLimits {
    fn default() -> Self {
        Self {
            concurrency: 64,
            per_user: 16,
            wait: Duration::from_secs(5),
        }
    }
}

impl DownloadLimits {
    /// The slots of public links' visitors, all links together and per link:
    /// fewer than the users', and separate from them.
    pub fn public() -> Self {
        Self {
            concurrency: 16,
            per_user: 4,
            wait: Duration::from_secs(5),
        }
    }

    pub fn slots(&self) -> Slots {
        Slots::new(self.concurrency, self.per_user, self.wait)
    }
}

/// A reader that keeps its download slot until it is dropped, i.e. until the
/// body finishes streaming or the client goes away.
pub struct HeldReader<R> {
    inner: R,
    _slot: Slot,
}

impl<R> HeldReader<R> {
    pub fn new(inner: R, slot: Slot) -> Self {
        Self { inner, _slot: slot }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for HeldReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncReadExt;
    use uuid::Uuid;

    use super::*;

    #[tokio::test]
    async fn the_slot_is_held_until_the_reader_is_dropped() {
        let slots = Slots::new(4, 1, Duration::from_millis(10));
        let user = Uuid::now_v7();
        let slot = slots.acquire(user).await.unwrap();
        let mut reader = HeldReader::new(&b"bytes"[..], slot);
        let mut read = Vec::new();
        reader.read_to_end(&mut read).await.unwrap();
        assert_eq!(read, b"bytes");
        assert!(
            slots.acquire(user).await.is_none(),
            "still held while alive"
        );
        drop(reader);
        assert!(slots.acquire(user).await.is_some());
    }
}
