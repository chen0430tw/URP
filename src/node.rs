#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum NodeType {
    Cpu,
    Gpu,
    Qcu,
    Memory,
    Network,
    Rule,
    Structure,
    /// USB-connected device (microcontroller, accelerator, etc.)
    /// URP routes packets to it via the UsbExecutor transport.
    Usb,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub node_id: String,
    pub node_type: NodeType,
    pub host_id: String,
    pub zone: String,
    pub tags: Vec<String>,
    pub compute_capacity: f32,
    pub memory_capacity: f32,
    pub bandwidth: f32,
    pub inertia_keys: Vec<String>,
    /// TCP address for remote execution: "host:port" (e.g. "192.168.1.2:7788").
    /// When set and host_id differs from the sending node, URXRuntime routes
    /// packets over real TCP instead of the stub path.
    pub address: Option<String>,
}

impl Node {
    pub fn new(node_id: &str, node_type: NodeType, compute_capacity: f32) -> Self {
        Self {
            node_id: node_id.to_string(),
            node_type,
            host_id: "default".to_string(),
            zone: "default".to_string(),
            tags: Vec::new(),
            compute_capacity,
            memory_capacity: 1024.0,
            bandwidth: 1000.0,
            inertia_keys: Vec::new(),
            address: None,
        }
    }

    /// Set the TCP address for remote routing.
    pub fn with_address(mut self, addr: impl Into<String>) -> Self {
        self.address = Some(addr.into());
        self
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    pub fn has_inertia_key(&self, key: &str) -> bool {
        self.inertia_keys.iter().any(|k| k == key)
    }

    pub fn remember_inertia_key(&mut self, key: &str) {
        if !self.has_inertia_key(key) {
            self.inertia_keys.push(key.to_string());
        }
    }
}
