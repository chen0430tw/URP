use crate::node::Node;

pub fn node_score(
    required_tag: &str,
    preferred_zone: &str,
    inertia_key: Option<&str>,
    node: &Node,
) -> f32 {
    let mut score = 0.0;
    if node.has_tag(required_tag) {
        score += 2.0;
    } else {
        return -1e9;
    }

    if node.zone == preferred_zone {
        score += 1.5;
    }

    score += 0.1 * node.compute_capacity;
    score += 0.02 * node.bandwidth;

    if let Some(key) = inertia_key {
        if node.has_inertia_key(key) {
            score += 3.0;
        }
    }

    score
}

pub fn route_cost(src: &Node, dst: &Node) -> f32 {
    let mut cost = 0.0;
    if src.host_id != dst.host_id {
        cost += 10.0;
    }
    if src.zone != dst.zone {
        cost += 3.0;
    }
    cost += (100.0 / dst.bandwidth.max(1.0)) * 0.1;
    cost
}
