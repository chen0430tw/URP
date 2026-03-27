use std::collections::{HashMap, HashSet};

use crate::cost::node_score;
use crate::node::Node;

pub trait SchedulerPolicy {
    fn select_partition_node(
        &self,
        required_tags: &HashSet<String>,
        preferred_zone: &str,
        inertia_key: Option<&str>,
        nodes: &HashMap<String, Node>,
    ) -> String;
}

#[derive(Debug, Clone)]
pub struct MultifactorPolicy {
    pub reservation_bias: f32,
}

impl MultifactorPolicy {
    pub fn new() -> Self {
        Self {
            reservation_bias: 0.5,
        }
    }
}

impl SchedulerPolicy for MultifactorPolicy {
    fn select_partition_node(
        &self,
        required_tags: &HashSet<String>,
        preferred_zone: &str,
        inertia_key: Option<&str>,
        nodes: &HashMap<String, Node>,
    ) -> String {
        nodes.values()
            .map(|n| {
                let mut score = 0.0f32;
                for tag in required_tags {
                    score += node_score(tag, preferred_zone, inertia_key, n);
                }
                score += self.reservation_bias * 0.0;
                (score, n.node_id.clone())
            })
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .expect("no node for partition")
            .1
    }
}
