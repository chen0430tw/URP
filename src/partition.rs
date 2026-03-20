use std::collections::{HashMap, HashSet};

use crate::ir::IRGraph;
use crate::node::Node;
use crate::policy::SchedulerPolicy;

pub fn bind_partitions<P: SchedulerPolicy>(
    graph: &IRGraph,
    partitions: &HashMap<String, String>,
    nodes: &HashMap<String, Node>,
    policy: &P,
) -> HashMap<String, String> {
    let mut part_to_blocks: HashMap<String, Vec<String>> = HashMap::new();
    for (bid, pid) in partitions {
        part_to_blocks.entry(pid.clone()).or_default().push(bid.clone());
    }

    let mut result = HashMap::new();

    for (pid, block_ids) in part_to_blocks {
        let blocks: Vec<_> = block_ids.iter().filter_map(|bid| graph.get_block(bid)).collect();

        let required_tags: HashSet<String> = blocks.iter().map(|b| b.required_tag.clone()).collect();
        let preferred_zone = blocks.first().map(|b| b.preferred_zone.clone()).unwrap_or_else(|| "default".into());
        let inertia_key = blocks.iter().find_map(|b| b.inertia_key.clone());

        let node_id = policy.select_partition_node(&required_tags, &preferred_zone, inertia_key.as_deref(), nodes);
        result.insert(pid, node_id);
    }

    result
}
