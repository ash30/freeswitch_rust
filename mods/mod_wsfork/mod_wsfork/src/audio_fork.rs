use anyhow::Result;
use fastwebsockets::{FragmentCollector, Frame, OpCode, WebSocket, WebSocketError};
use http_body_util::Empty;
use hyper::{
    Request,
    body::Bytes,
    header::{CONNECTION, UPGRADE},
};
use std::ops::DerefMut;
use std::sync::Arc;
use thingbuf::Recycle;
use thingbuf::mpsc::errors::TrySendError;
use tokio::pin;
use tokio::sync::Notify;
use url::Url;

const CANCEL_REASON: &str = "LOCAL_CANCEL";
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
        element.fill(0);
    }
    fn new_element(&self) -> DataBuffer {
        vec![0; self.0]
    }
}

pub enum WSForkerError {
    Full,
    Closed,
}
impl From<TrySendError> for WSForkerError {
    fn from(value: TrySendError) -> Self {
        match value {
            TrySendError::Full(_) => Self::Full,
            _ => Self::Closed,
        }
    }
}

pub fn new_wsfork(
    url: url::Url,
    frame_size: usize,
    buffer_duration: usize,
    headers: impl FnOnce(&mut WSRequest),
) -> Result<(WSForkSender, WSForkReceiver)> {
    let (tx, rx) = thingbuf::mpsc::with_recycle(buffer_duration, DataBufferFactory(frame_size));

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

pub struct WSForkReceiver {
    rx: thingbuf::mpsc::Receiver<DataBuffer, DataBufferFactory>,
    pub req: WSRequest,
    cancel: Arc<Notify>,
}

impl WSForkReceiver {
    pub async fn run<S>(
        self,
        ws: WebSocket<S>,
        on_event: impl Fn(wsfork_events::Body) + Clone,
    ) -> std::result::Result<(), WebSocketError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let res = self.run_loop(ws, on_event.clone()).await;
        match &res {
            Ok(()) => {}
            Err(e) => on_event(wsfork_events::Body::Error {
                desc: format!("{:#}", e),
            }),
        }
        res
    }

    async fn run_loop<S>(
        self,
        mut ws: WebSocket<S>,
        on_event: impl Fn(wsfork_events::Body),
    ) -> std::result::Result<(), WebSocketError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
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
                        break 'outer Ok(None)
                    }
                    next_send = &mut fut => {
                        if let Some(frame) = next_send {
                            ws.write_frame(Frame::binary(fastwebsockets::Payload::Borrowed(
                                frame.as_slice(),
                            ))).await?;
                            break
                        }
                        else {
                            break 'outer Err(());
                        }
                    }
                    next_recv = ws.read_frame() => {
                        let Frame { opcode, payload, .. } = next_recv?;
                        match opcode {
                            OpCode::Close => {
                                break 'outer Ok(Some(payload))
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
            Err(_) => {
                // Sender dropped without cancel... shouldn't happen!
                let _ = ws
                    .write_frame(Frame::close(1001, CANCEL_REASON.as_bytes()))
                    .await;
                (Some(1001), Some(CANCEL_REASON.to_string()))
            }
            Ok(None) => {
                // Sender was cancelled ie graceful shutdown
                let _ = ws
                    .write_frame(Frame::close(1000, CANCEL_REASON.as_bytes()))
                    .await;
                (Some(1000), Some(CANCEL_REASON.to_string()))
            }
            Ok(Some(p)) => match p.len() {
                // remote closed
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

pub struct WSForkSender {
    tx: thingbuf::mpsc::Sender<DataBuffer, DataBufferFactory>,
    cancel: Arc<Notify>,
}

impl WSForkSender {
    pub fn get_next_free_buffer(
        &self,
    ) -> std::result::Result<impl DerefMut<Target = DataBuffer>, WSForkerError> {
        self.tx.try_send_ref().map_err(|e| e.into())
    }

    pub fn cancel(&self) {
        self.cancel.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use url::Url;

    #[derive(Clone)]
    struct EventCollector {
        events: Arc<Mutex<Vec<wsfork_events::Body>>>,
    }

    impl EventCollector {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn collect(&self, event: wsfork_events::Body) {
            self.events.lock().unwrap().push(event);
        }

        fn inspect_event(&self, index: usize) -> Option<wsfork_events::Body> {
            self.events.lock().unwrap().get(index).cloned()
        }

        fn event_count(&self) -> usize {
            self.events.lock().unwrap().len()
        }
    }

    mod websocket_tests {
        use super::*;
        use fastwebsockets::WebSocket;
        use tokio_test::io::Builder;

        #[tokio::test]
        async fn test_websocket_connected_then_cancel() {
            let url = Url::parse("ws://localhost:8080/test").unwrap();
            let (sender, receiver) =
                new_wsfork(url, 1024, Duration::from_millis(100), |_req| {}).unwrap();

            let mock_stream = Builder::new()
                // expect a close frame + 1000
                .write(b"\x88\x0E\x03\xE8LOCAL_CANCEL")
                .wait(Duration::from_secs(5))
                .build();

            let mut mock_ws = WebSocket::after_handshake(mock_stream, fastwebsockets::Role::Client);
            mock_ws.set_auto_apply_mask(false);

            let event_collector = EventCollector::new();
            let collector_clone = event_collector.clone();

            let handle = tokio::spawn(async move {
                receiver
                    .run(mock_ws, move |event| collector_clone.collect(event))
                    .await
            });

            sender.cancel();
            let result = handle.await;

            assert!(matches!(
                event_collector.inspect_event(0),
                Some(wsfork_events::Body::Connected {})
            ));

            assert!(matches!(
                event_collector.inspect_event(1),
                Some(wsfork_events::Body::Closed { code, .. }) if code == Some(1000)
            ));

            assert_eq!(event_collector.event_count(), 2);
            assert!(result.is_ok());

            assert!(sender.get_next_free_buffer().is_err());
        }

        #[tokio::test]
        async fn test_websocket_connected_then_io_error() {
            let url = Url::parse("ws://localhost:8080/test").unwrap();
            let (sender, receiver) =
                new_wsfork(url, 1024, Duration::from_millis(100), |_req| {}).unwrap();

            let mock_stream = Builder::new()
                .read_error(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "UnexpectedEof",
                ))
                .build();
            let mock_ws = WebSocket::after_handshake(mock_stream, fastwebsockets::Role::Client);

            let event_collector = EventCollector::new();
            let collector_clone = event_collector.clone();

            let result = receiver
                .run(mock_ws, move |event| collector_clone.collect(event))
                .await;

            assert!(matches!(
                event_collector.inspect_event(0),
                Some(wsfork_events::Body::Connected {})
            ));

            assert!(matches!(
                event_collector.inspect_event(1),
                Some(wsfork_events::Body::Error { ref desc })
                if desc.contains("UnexpectedEof")
            ));

            assert_eq!(event_collector.event_count(), 2);
            assert!(result.is_err());
            // TODO: assert errors type

            // Sender is now closed, to help inform FS
            assert!(sender.get_next_free_buffer().is_err());
        }
    }
}
