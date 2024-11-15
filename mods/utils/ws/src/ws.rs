use anyhow::Result;
use tungstenite::{accept, WebSocket};
use ringbuf::{traits::*, LocalRb, storage::Heap};
use std::net::{IpAddr, SocketAddr};

pub struct WSForker {
    ws: Option<WebSocket<std::net::TcpStream>>,
    buf: LocalRb<Heap<u8>>
}

impl WSForker {
    pub fn new(size:usize) -> Self {
        Self {
            ws: None,
            buf: LocalRb::new(size)
        }
    }

    // Life-cycle methods 
    pub fn connect(&mut self, ip:String, port:u16) -> Result<()> {
        let ip_addr = IpAddr::V4(ip.parse()?);
        let addr = SocketAddr::new(ip_addr, port);
        let stream = std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(1))?;
        self.ws = Some(accept(stream)?);
        Ok(())
    }

    pub fn fork(&mut self, data:&[u8]) -> bool {
        let Some(ws) = self.ws.as_mut()else { return true };
        self.buf.push_slice_overwrite(data);

        // TODO: REALLY! want to remove this allocation ...
        let v:Vec<u8> = self.buf.pop_iter().collect();
        match ws.send(tungstenite::Message::binary(v)) {
            Err(tungstenite::Error::ConnectionClosed) => {
                // TODO Send event
                return false 
            },
            Err(tungstenite::Error::Io(e)) => if let std::io::ErrorKind::WouldBlock = e.kind() { 
                // Shouldn't get here...
            }
            Err(tungstenite::Error::WriteBufferFull(_)) => {
                // drop packets...
            },
            Err(_) => {
                // All other errors are considered fatal
                return false 
            },
            Ok(_) => {
                // continue 
            }
        }   
        true
    }

    pub fn close(&mut self) {
        let Some(ws) = self.ws.as_mut()else { return };
        let _ =  ws.close(None);
    }

}

