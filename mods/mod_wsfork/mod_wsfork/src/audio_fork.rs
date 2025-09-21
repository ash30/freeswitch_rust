use anyhow::Result;
use fastwebsockets::Frame;
use fastwebsockets::handshake;
use http_body_util::Empty;
use hyper::{
    Request,
    body::Bytes,
    header::{CONNECTION, UPGRADE},
};
use url::Url;

use std::io::Error;
use std::io::Read;
use std::pin::Pin;
use std::time::Duration;
use thingbuf::Recycle;
use thingbuf::mpsc::errors::TrySendError;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;

pub type WSRequest = Request<Empty<Bytes>>;

fn create_request(url: Url) -> Result<WSRequest> {
    Request::builder()
        .method("GET")
        .uri(url.as_str())
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
type DataBuffer = Vec<u8>;
struct DataBufferFactory(usize);

impl Recycle<DataBuffer> for DataBufferFactory {
    fn recycle(&self, element: &mut DataBuffer) {
        todo!()
    }
    fn new_element(&self) -> DataBuffer {
        Vec::with_capacity(self.0)
    }
}

pub enum WSForkerError {
    Full,
    Closed,
    ReadError(Error),
}
impl From<TrySendError> for WSForkerError {
    fn from(value: TrySendError) -> Self {
        match value {
            TrySendError::Full(_) => Self::Full,
            _ => Self::Closed,
        }
    }
}

const PACKETIZATION_PERIOD: u32 = 20; // ms 

pub fn new_wsfork(
    url: url::Url,
    frame_size: usize,
    buf_duration: Duration, // TODO: change to time format
    headers: impl FnOnce(&mut WSRequest),
) -> Result<(WSForkSender, WSForkReceiver)> {
    let buffer_frame_size = buf_duration.as_millis().max(100).min(20) as u32 / PACKETIZATION_PERIOD;
    let (tx, rx) =
        thingbuf::mpsc::with_recycle(buffer_frame_size as usize, DataBufferFactory(frame_size));

    let mut req = create_request(url)?;
    (headers(&mut req));

    Ok((WSForkSender { tx }, WSForkReceiver { req, rx }))
}

pub struct WSForkSender {
    tx: thingbuf::mpsc::Sender<DataBuffer, DataBufferFactory>,
}

pub struct WSForkReceiver {
    rx: thingbuf::mpsc::Receiver<DataBuffer, DataBufferFactory>,
    req: WSRequest,
}

impl WSForkReceiver {
    pub async fn run<S, E>(
        self,
        stream: S,
        executor: E,
        on_event: impl Fn(wsfork_events::Body),
    ) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
        // IE something that can spawn tasks
        E: hyper::rt::Executor<Pin<Box<dyn Future<Output = ()> + Send>>>,
    {
        let (mut ws, _) = handshake::client(&executor, self.req, stream).await?;
        ws.set_auto_close(true);
        ws.set_writev(true);
        on_event(wsfork_events::Body::Connected {});

        loop {
            let Some(frame) = self.rx.recv_ref().await else {
                let reason = "";
                let _ = ws.write_frame(Frame::close(1000, reason.as_bytes())).await;
                break;
            };

            // Write errors will early return, we assume ws is foobar'd
            ws.write_frame(Frame::binary(fastwebsockets::Payload::Borrowed(
                frame.as_slice(),
            )))
            .await?;
        }

        on_event(wsfork_events::Body::Closed {});
        Ok(())
    }
}

impl WSForkSender {
    pub fn send_frame(&self, mut src: impl Read) -> std::result::Result<(), WSForkerError> {
        let mut send_ref = self.tx.try_send_ref()?;

        // TODO: Fix buf read
        src.read(&mut send_ref).map_err(WSForkerError::ReadError)?;

        // on drop, we notify receiver
        Ok(())
    }
}
