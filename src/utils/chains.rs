use smol::{io::AsyncRead, ready};
use std::fmt;
use std::io::IoSliceMut;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Chains<R> {
    readers: Vec<R>,
    last_active: usize,
}

impl<R> Chains<R> {
    pub fn new(readers: Vec<R>) -> Self {
        assert!(!readers.is_empty());
        let last_active = 0;
        Self {
            readers,
            last_active,
        }
    }
}

impl<R: fmt::Debug> fmt::Debug for Chains<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Chains").field("r", &self.readers).finish()
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for Chains<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize>> {
        loop {
            let last_active = self.last_active;
            let max_last_active = self.readers.len() - 1;
            let readers: &mut R = self.readers.get_mut(last_active).unwrap();
            match ready!(Pin::new(readers).poll_read(cx, buf)) {
                Ok(0) if !buf.is_empty() => {
                    if last_active == max_last_active {
                        return Poll::Ready(Ok(0));
                    }
                    if last_active < max_last_active {
                        self.last_active += 1;
                    }
                }
                Ok(n) => return Poll::Ready(Ok(n)),
                Err(err) => return Poll::Ready(Err(err)),
            }
        }
    }

    fn poll_read_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
    ) -> Poll<Result<usize>> {
        loop {
            let last_active = self.last_active;
            let max_last_active = self.readers.len() - 1;
            let readers: &mut R = self.readers.get_mut(last_active).unwrap();
            match ready!(Pin::new(readers).poll_read_vectored(cx, bufs)) {
                Ok(0) if !bufs.is_empty() => {
                    if last_active == max_last_active {
                        return Poll::Ready(Ok(0));
                    }
                    if last_active < max_last_active {
                        self.last_active += 1;
                    }
                }
                Ok(n) => return Poll::Ready(Ok(n)),
                Err(err) => return Poll::Ready(Err(err)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smol::{block_on, io::AsyncReadExt};

    #[test]
    fn test_chains() {
        let input_a: &[u8] = b"hello";
        let input_b: &[u8] = b"world";
        let mut chains = Chains::new(vec![input_a, input_b]);
        let mut buf = Vec::new();
        block_on(async move {
            let result = chains.read_to_end(&mut buf).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), input_a.len() + input_b.len());
            assert_eq!(buf.as_slice(), &b"helloworld"[..]);
        })
    }
}
