use anyhow::Result;
use fastwebsockets::FragmentCollector;
use fastwebsockets::Frame;
use fastwebsockets::OpCode;
use fastwebsockets::handshake;
use http_body_util::Empty;
use hyper::{
    Request,
    body::Bytes,
    header::{CONNECTION, UPGRADE},
};
use tokio::sync::Notify;
use url::Url;

use std::io::Error;
use std::io::Read;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use thingbuf::Recycle;
use thingbuf::mpsc::errors::TrySendError;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::pin;

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

    let cancel = Arc::new(Notify::new());

    Ok((
        WSForkSender {
            tx,
            cancel: cancel.clone(),
        },
        WSForkReceiver { req, rx, cancel },
    ))
}

pub struct WSForkSender {
    tx: thingbuf::mpsc::Sender<DataBuffer, DataBufferFactory>,
    cancel: Arc<Notify>,
}

pub struct WSForkReceiver {
    rx: thingbuf::mpsc::Receiver<DataBuffer, DataBufferFactory>,
    req: WSRequest,
    cancel: Arc<Notify>,
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
        ws.set_auto_pong(true);
        ws.set_writev(true);
        let mut ws = FragmentCollector::new(ws);
        on_event(wsfork_events::Body::Connected {});

        let close = 'outer: loop {
            let fut = self.rx.recv_ref();
            pin!(fut);

            let cancel = self.cancel.notified();
            pin!(cancel);

            loop {
                tokio::select! {
                    // FS should cancel via notify
                    // and the channel is only dropped if cleaning up an irregular case
                    // either way exit the loop and notify WS
                    _ = &mut cancel => {
                        break 'outer None
                    }
                    next_send = &mut fut => {
                        if let Some(frame) = next_send {
                            ws.write_frame(Frame::binary(fastwebsockets::Payload::Borrowed(
                                frame.as_slice(),
                            ))).await?;
                            break
                        }
                        else {
                            break 'outer None;
                        }
                    }
                    next_recv = ws.read_frame() => {
                        let Frame { opcode, payload, .. } = next_recv?;
                        match opcode {
                            OpCode::Close => {
                                break 'outer Some(payload)
                            },
                            OpCode::Text => {
                                let content = String::from_utf8(payload.to_owned())
                                    .unwrap_or_default();
                                on_event(wsfork_events::Body::Message { content })
                            },
                            OpCode::Binary => {
                                // NOT supported atm
                            },
                            _ => {}
                        }
                    }
                }
            }
        };

        let (code, reason) = match close {
            None => {
                let reason = "LOCAL CANCEL";
                let _ = ws.write_frame(Frame::close(1000, reason.as_bytes())).await;
                (None, Some(reason.to_string()))
            }
            Some(p) => match p.len() {
                0 => (None, None),
                1 => (Some(1002), None),
                2 => (Some(u16::from_be_bytes([p[0], p[1]])), None),
                _ => (
                    Some(u16::from_be_bytes([p[0], p[1]])),
                    String::from_utf8(p[2..].to_vec()).ok(),
                ),
            },
        };

        on_event(wsfork_events::Body::Closed { code, reason });
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

    pub fn cancel(&self) {
        self.cancel.notify_waiters();
    }
}
