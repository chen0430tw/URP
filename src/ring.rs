use tokio::sync::mpsc;

use crate::packet::URPPacket;

#[derive(Debug)]
pub struct LocalRingTunnel {
    tx: mpsc::Sender<URPPacket>,
    rx: mpsc::Receiver<URPPacket>,
}

impl LocalRingTunnel {
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self { tx, rx }
    }

    pub async fn push(&self, packet: URPPacket) {
        self.tx.send(packet).await.expect("ring push failed");
    }

    pub async fn pop(&mut self) -> URPPacket {
        self.rx.recv().await.expect("ring pop failed")
    }
}
