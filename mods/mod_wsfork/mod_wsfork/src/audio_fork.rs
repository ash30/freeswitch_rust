use anyhow::Result;
use fastwebsockets::Frame;
use fastwebsockets::handshake;
use http_body_util::Empty;
use hyper::{
    Request,
    body::Bytes,
    header::{CONNECTION, UPGRADE},
};

use hyper_util::rt::TokioExecutor;
use ringbuf::HeapRb;
use ringbuf::traits::Consumer;
use ringbuf::traits::Producer;
use ringbuf::traits::Split;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Read;
use std::mem::MaybeUninit;
use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::sync::Notify;

type WSRequest = Request<Empty<Bytes>>;

pub fn create_request(url: String, customise: impl FnOnce(&mut WSRequest)) -> Result<WSRequest> {
    Request::builder()
        .method("GET")
        .uri(url)
        .header(UPGRADE, "websocket")
        .header(CONNECTION, "upgrade")
        .header(
            "Sec-WebSocket-Key",
            fastwebsockets::handshake::generate_key(),
        )
        .header("Sec-WebSocket-Version", "13")
        .body(Empty::new())
        .map_err(|e| e.into())
}

pub struct AudioFork<B> {
    req: WSRequest,
    read: Arc<Notify>,
    stop: Arc<Notify>,
    buf: B,
}

// Note: 4 candles!!!
pub struct AudioForkHandle<B> {
    read: Arc<Notify>,
    stop: Arc<Notify>,
    buf: B,
}

impl<B> AudioForkHandle<B>
where
    B: Producer<Item = u8>,
{
    pub fn copy_samples<R>(&mut self, src: &mut R, size: usize) -> std::io::Result<()>
    where
        R: Read,
    {
        if self.buf.vacant_len() < size {
            return Err(Error::new(ErrorKind::UnexpectedEof, ""));
        }
        let (left, right) = self.buf.vacant_slices_mut();

        let mut remaining = size;
        for slice in vec![left, right] {
            let start = slice.len();
            let buf = &mut slice[..start];
            buf.fill(MaybeUninit::new(0));
            unsafe {
                let buf = &mut *(slice as *mut [MaybeUninit<u8>] as *mut [u8]);
                let n = src.read(buf)?;
                remaining -= n;
                if remaining == 0 {
                    break;
                };
            }
        }
        if remaining != 0 {
            return Err(Error::new(ErrorKind::Other, ""));
        }
        unsafe { self.buf.advance_write_index(size) };
        self.read.notify_waiters();

        Ok(())
    }

    pub fn close(&self) {
        self.stop.notify_waiters();
    }
}

pub fn new_fork(
    req: WSRequest,
    buf_size: usize,
) -> (
    AudioForkHandle<impl Producer<Item = u8>>,
    AudioFork<impl Consumer<Item = u8>>,
) {
    let stop = Arc::new(Notify::new());
    let read = Arc::new(Notify::new());

    let rb = HeapRb::<u8>::new(buf_size);
    let (buf_tx, buf_rx) = rb.split();

    (
        AudioForkHandle {
            buf: buf_tx,
            stop: stop.clone(),
            read: read.clone(),
        },
        AudioFork {
            req,
            stop: stop.clone(),
            read: read.clone(),
            buf: buf_rx,
        },
    )
}

impl<B> AudioFork<B>
where
    B: Consumer<Item = u8>,
{
    pub async fn run<S>(self, stream: S) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        let exector = TokioExecutor::new();
        let (mut ws, res) = handshake::client(&exector, self.req, stream).await?;
        ws.set_auto_close(true);

        loop {
            tokio::select! {
                _ = self.stop.notified() => {
                    // TODO: WHAT REASON
                    ws.write_frame(Frame::close(0, b"")).await;
                    break
                },
                _ = self.read.notified() => {
                    // READ ALL
                    let slices = self.buf.as_slices();
                    for b in [slices.0, slices.1] {
                        // how much samples DO we want to send ?
                        let sample_rate = 8000;
                        let frame_payload_size = 2 * ( sample_rate / 1000) * 20;
                        b.chunks(frame_payload_size).for_each( | data | {
                            // TODO:
                            ws.write_frame(Frame::binary(fastwebsockets::Payload::Borrowed(data)));
                            unsafe {
                                self.buf.advance_read_index(data.len());
                            }

                        });
                    }

                }
            }
        }

        Ok(())
    }
}
