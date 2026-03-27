#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeType {
    Cpu,
    Gpu,
    Qcu,
    Memory,
    Network,
    Rule,
    Structure,
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
        }
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
