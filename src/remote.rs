use crate::packet::URPPacket;

#[derive(Debug, Default)]
pub struct RemotePacketLink {
    pub sent_packets: usize,
}

impl RemotePacketLink {
    pub fn new() -> Self {
        Self { sent_packets: 0 }
    }

    pub async fn send(&mut self, packet: URPPacket) -> URPPacket {
        self.sent_packets += 1;
        packet
    }
}
