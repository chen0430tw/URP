use crate::packet::URPPacket;
use bytes::{Buf, BufMut, BytesMut};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use futures::{SinkExt, StreamExt};
use tokio_util::codec::{Decoder, Encoder, Framed};

/// Remote packet link configuration
#[derive(Debug, Clone)]
pub struct LinkConfig {
    /// Maximum frame size in bytes
    pub max_frame_size: usize,
    /// Connection timeout in seconds
    pub timeout_secs: u64,
    /// Maximum number of connections per host
    pub max_connections: usize,
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self {
            max_frame_size: 8 * 1024 * 1024, // 8 MB
            timeout_secs: 30,
            max_connections: 10,
        }
    }
}

/// Packet codec for framing URPPacket over TCP
#[derive(Debug)]
pub struct PacketCodec {
    max_frame_size: usize,
}

impl PacketCodec {
    pub fn new(max_frame_size: usize) -> Self {
        Self { max_frame_size }
    }
}

impl Decoder for PacketCodec {
    type Item = Vec<u8>;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            return Ok(None);
        }

        let length = u32::from_be_bytes([src[0], src[1], src[2], src[3]]) as usize;

        if length > self.max_frame_size {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame too large: {} bytes", length),
            ));
        }

        if src.len() < 4 + length {
            src.reserve(4 + length - src.len());
            return Ok(None);
        }

        let data = src[4..4 + length].to_vec();
        src.advance(4 + length);

        Ok(Some(data))
    }
}

impl Encoder<Vec<u8>> for PacketCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: Vec<u8>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let length = item.len() as u32;
        dst.put_slice(&length.to_be_bytes());
        dst.put_slice(&item);
        Ok(())
    }
}

/// A single TCP connection with framed transport
#[derive(Debug)]
pub struct Connection {
    #[allow(dead_code)]
    addr: String,
    framed: Framed<TcpStream, PacketCodec>,
}

impl Connection {
    /// Create a new connection to the given address
    pub async fn connect(addr: &str, config: &LinkConfig) -> Result<Self, std::io::Error> {
        let stream = TcpStream::connect(addr).await?;
        let codec = PacketCodec::new(config.max_frame_size);
        let framed = Framed::new(stream, codec);

        Ok(Self {
            addr: addr.to_string(),
            framed,
        })
    }

    /// Send a packet and wait for response
    pub async fn send_packet(&mut self, packet: URPPacket) -> Result<URPPacket, Box<dyn std::error::Error>> {
        let bytes = packet.to_bytes();
        self.framed.send(bytes).await?;

        match self.framed.next().await {
            Some(Ok(response_bytes)) => Ok(URPPacket::from_bytes(&response_bytes)?),
            Some(Err(e)) => Err(Box::new(e) as Box<dyn std::error::Error>),
            None => Err("Connection closed".into()),
        }
    }

    /// Send multiple packets in batch
    pub async fn send_batch(&mut self, packets: Vec<URPPacket>) -> Result<Vec<URPPacket>, Box<dyn std::error::Error>> {
        let mut results = Vec::with_capacity(packets.len());

        for packet in packets {
            let result = self.send_packet(packet).await?;
            results.push(result);
        }

        Ok(results)
    }
}

/// Remote packet link with connection pooling
#[derive(Debug)]
pub struct RemotePacketLink {
    connections: Arc<Mutex<HashMap<String, Arc<Mutex<Connection>>>>>,
    config: LinkConfig,
    pub sent_packets: usize,
}

impl RemotePacketLink {
    pub fn new() -> Self {
        Self::with_config(LinkConfig::default())
    }

    pub fn with_config(config: LinkConfig) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            config,
            sent_packets: 0,
        }
    }

    /// Get or create a connection to the given address
    async fn get_connection(&self, addr: &str) -> Result<Arc<Mutex<Connection>>, Box<dyn std::error::Error>> {
        let mut connections = self.connections.lock().await;

        if let Some(conn) = connections.get(addr) {
            Ok(conn.clone())
        } else {
            if connections.len() >= self.config.max_connections {
                return Err("Maximum connections reached".into());
            }

            let conn = Connection::connect(addr, &self.config).await?;
            let conn = Arc::new(Mutex::new(conn));
            connections.insert(addr.to_string(), conn.clone());
            Ok(conn)
        }
    }

    /// Send a packet to a remote node
    pub async fn send(&mut self, addr: &str, packet: URPPacket) -> Result<URPPacket, Box<dyn std::error::Error>> {
        let conn = self.get_connection(addr).await?;
        let mut conn_guard = conn.lock().await;
        let result = conn_guard.send_packet(packet).await?;
        self.sent_packets += 1;
        Ok(result)
    }

    /// Send multiple packets to multiple remote nodes
    pub async fn send_batch_to(
        &mut self,
        packets: Vec<(String, URPPacket)>,
    ) -> Result<Vec<URPPacket>, Box<dyn std::error::Error>> {
        let mut results = Vec::with_capacity(packets.len());

        for (addr, packet) in packets {
            let result = self.send(&addr, packet).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Close all connections
    pub async fn close_all(&self) {
        let mut connections = self.connections.lock().await;
        connections.clear();
    }

    /// Get number of active connections
    pub async fn connection_count(&self) -> usize {
        self.connections.lock().await.len()
    }
}

impl Default for RemotePacketLink {
    fn default() -> Self {
        Self::new()
    }
}

// Keep backward compatibility with the old simple API
impl RemotePacketLink {
    pub async fn send_legacy(&mut self, packet: URPPacket) -> URPPacket {
        self.sent_packets += 1;
        // Stub: no address configured, pass packet through unchanged.
        packet
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TCP server — receive URPPackets, process them, send back responses
// ─────────────────────────────────────────────────────────────────────────────

impl RemotePacketLink {
    /// Start a TCP server at `addr` that processes incoming `URPPacket`s with
    /// the provided `handler` closure and sends the returned packet back as the
    /// response.
    ///
    /// # Example
    /// ```no_run
    /// use urx_runtime_v08::remote::RemotePacketLink;
    /// use urx_runtime_v08::packet::{PayloadCodec, PayloadValue, URPPacket};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     RemotePacketLink::serve("0.0.0.0:7788", |pkt| {
    ///         // Echo the packet back unchanged (real impl would exec the opcode)
    ///         pkt
    ///     }).await.unwrap();
    /// }
    /// ```
    pub async fn serve<F>(addr: &str, handler: F) -> Result<(), std::io::Error>
    where
        F: Fn(URPPacket) -> URPPacket + Send + Sync + 'static + Clone,
    {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind(addr).await?;
        eprintln!("[RemotePacketLink] Listening on {}", addr);

        loop {
            let (stream, peer) = listener.accept().await?;
            eprintln!("[RemotePacketLink] Connection from {}", peer);
            let handler = handler.clone();
            let config = LinkConfig::default();

            tokio::spawn(async move {
                let mut framed = Framed::new(stream, PacketCodec::new(config.max_frame_size));
                while let Some(frame) = framed.next().await {
                    match frame {
                        Ok(bytes) => match URPPacket::from_bytes(&bytes) {
                            Ok(packet) => {
                                let response = handler(packet);
                                if let Err(e) = framed.send(response.to_bytes()).await {
                                    eprintln!("[RemotePacketLink] Send error: {e}");
                                    break;
                                }
                            }
                            Err(e) => eprintln!("[RemotePacketLink] Decode error: {e}"),
                        },
                        Err(e) => {
                            eprintln!("[RemotePacketLink] Frame error: {e}");
                            break;
                        }
                    }
                }
            });
        }
    }
}
