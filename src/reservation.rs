//! Resource Reservation and Backfill System
//!
//! This module implements time-based resource reservation with backfill support:
//! - Reservations are time-based allocations of nodes to partitions
//! - Backfill allows short tasks to run in gaps between reservations
//! - Policy integration enables scheduler-aware reservation strategies

use std::collections::{HashMap, BTreeMap, HashSet};
use std::ops::Range;

/// A time-based reservation of a node for a partition
#[derive(Debug, Clone)]
pub struct Reservation {
    pub partition_id: String,
    pub node_id: String,
    pub start_time: u32,
    pub end_time: u32,
    pub priority: ReservationPriority,
    pub resource_shape: String,
}

impl Reservation {
    pub fn new(partition_id: String, node_id: String, start_time: u32, end_time: u32) -> Self {
        Self {
            partition_id,
            node_id,
            start_time,
            end_time,
            priority: ReservationPriority::Normal,
            resource_shape: String::new(),
        }
    }

    pub fn with_priority(mut self, priority: ReservationPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_resource(mut self, shape: &str) -> Self {
        self.resource_shape = shape.to_string();
        self
    }

    /// Get the duration of this reservation
    pub fn duration(&self) -> u32 {
        self.end_time.saturating_sub(self.start_time)
    }

    /// Check if this reservation overlaps with a time range
    pub fn overlaps(&self, time: u32, duration: u32) -> bool {
        let end = time.saturating_add(duration);
        time < self.end_time && end > self.start_time
    }

    /// Check if this reservation is active at a given time
    pub fn is_active_at(&self, time: u32) -> bool {
        time >= self.start_time && time < self.end_time
    }

    /// Check if this reservation can accommodate a backfill task
    pub fn can_backfill(&self, now: u32, duration: u32) -> bool {
        // Backfill allowed if:
        // 1. Task is short (<= 20% of reservation duration or <= 5 time units)
        // 2. Task fits in available time before reservation starts OR after it ends
        let max_backfill_duration = std::cmp::max(
            self.duration() / 5,
            std::cmp::min(5, self.duration())
        );

        if duration > max_backfill_duration {
            return false;
        }

        // Allow tasks that complete before reservation starts
        if now.saturating_add(duration) <= self.start_time {
            return true;
        }

        // Allow tasks that start after reservation ends
        if now >= self.end_time {
            return true;
        }

        false
    }
}

/// Priority level for reservations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReservationPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Table of reservations with time-based indexing
#[derive(Debug, Default)]
pub struct ReservationTable {
    // All reservations, stored for iteration
    items: Vec<Reservation>,

    // Time-based index: start_time -> list of reservations starting at that time
    by_start_time: BTreeMap<u32, Vec<usize>>,

    // Node-based index: node_id -> list of reservation indices
    by_node: HashMap<String, Vec<usize>>,

    // Current time for the table
    current_time: u32,
}

impl ReservationTable {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            by_start_time: BTreeMap::new(),
            by_node: HashMap::new(),
            current_time: 0,
        }
    }

    pub fn with_current_time(mut self, time: u32) -> Self {
        self.current_time = time;
        self
    }

    /// Add a reservation to the table
    pub fn add(&mut self, r: Reservation) {
        let idx = self.items.len();

        // Index by start time
        self.by_start_time
            .entry(r.start_time)
            .or_insert_with(Vec::new)
            .push(idx);

        // Index by node
        self.by_node
            .entry(r.node_id.clone())
            .or_insert_with(Vec::new)
            .push(idx);

        self.items.push(r);
    }

    /// Remove all reservations that have ended before current time
    pub fn cleanup_expired(&mut self) {
        let mut to_remove = Vec::new();

        for (idx, r) in self.items.iter().enumerate() {
            if r.end_time <= self.current_time {
                to_remove.push(idx);
            }
        }

        // Rebuild indexes without removed reservations
        self.rebuild_indexes(&to_remove);
    }

    fn rebuild_indexes(&mut self, remove_indices: &[usize]) {
        let remove_set: HashSet<usize> = remove_indices.iter().cloned().collect();

        // Filter items
        let old_items = std::mem::take(&mut self.items);
        self.items = old_items
            .into_iter()
            .enumerate()
            .filter(|(idx, _)| !remove_set.contains(idx))
            .map(|(_, item)| item)
            .collect();

        // Rebuild indexes
        self.by_start_time.clear();
        self.by_node.clear();

        for (idx, r) in self.items.iter().enumerate() {
            self.by_start_time
                .entry(r.start_time)
                .or_insert_with(Vec::new)
                .push(idx);

            self.by_node
                .entry(r.node_id.clone())
                .or_insert_with(Vec::new)
                .push(idx);
        }
    }

    /// Find the earliest time a task can start considering all reservations
    pub fn earliest_start_time(&self, node_id: &str, duration: u32, preferred_time: u32) -> Option<u32> {
        let _reservations = self.by_node.get(node_id)?;

        // Check if preferred time works
        if self.can_schedule_at(node_id, preferred_time, duration) {
            return Some(preferred_time);
        }

        // Binary search for next available slot
        let mut search_time = preferred_time;

        for _ in 0..10 { // Limit iterations to avoid infinite loops
            if self.can_schedule_at(node_id, search_time, duration) {
                return Some(search_time);
            }

            // Find next reservation end time
            let next_end = self.next_reservation_end(node_id, search_time);
            search_time = next_end.unwrap_or(search_time + duration);
        }

        None
    }

    /// Check if a task can be scheduled at a specific time
    pub fn can_schedule_at(&self, node_id: &str, time: u32, duration: u32) -> bool {
        if let Some(reservations) = self.by_node.get(node_id) {
            for &idx in reservations {
                let r = &self.items[idx];
                if r.overlaps(time, duration) {
                    return false;
                }
            }
        }
        true
    }

    /// Find the next reservation end time after a given time
    fn next_reservation_end(&self, node_id: &str, after_time: u32) -> Option<u32> {
        self.by_node.get(node_id)?
            .iter()
            .filter_map(|&idx| {
                let r = &self.items[idx];
                if r.start_time >= after_time {
                    Some(r.end_time)
                } else {
                    None
                }
            })
            .min()
    }

    /// Find backfill opportunities - gaps between reservations
    pub fn find_backfill_windows(&self, node_id: &str, max_duration: u32) -> Vec<BackfillWindow> {
        let mut windows = Vec::new();

        if let Some(reservations) = self.by_node.get(node_id) {
            let mut sorted_reservations: Vec<_> = reservations
                .iter()
                .map(|&idx| &self.items[idx])
                .collect();
            sorted_reservations.sort_by_key(|r| r.start_time);

            let mut current_time = self.current_time;

            for r in sorted_reservations {
                if r.start_time > current_time {
                    let gap_duration = r.start_time - current_time;
                    if gap_duration >= 1 && gap_duration <= max_duration {
                        windows.push(BackfillWindow {
                            node_id: node_id.to_string(),
                            start_time: current_time,
                            end_time: r.start_time,
                            duration: gap_duration,
                            max_priority: ReservationPriority::Normal,
                        });
                    }
                }
                current_time = std::cmp::max(current_time, r.end_time);
            }

            // Window after last reservation
            let future_duration = max_duration.min(100); // Cap future window
            windows.push(BackfillWindow {
                node_id: node_id.to_string(),
                start_time: current_time,
                end_time: current_time + future_duration,
                duration: future_duration,
                max_priority: ReservationPriority::Low,
            });
        }

        windows
    }

    /// Check if a backfill task can run now
    pub fn can_backfill_now(&self, node_id: &str, duration: u32) -> bool {
        // Check against current reservations
        if let Some(reservations) = self.by_node.get(node_id) {
            for &idx in reservations {
                let r = &self.items[idx];
                if r.is_active_at(self.current_time) {
                    // Reservation is active, check if backfill allowed
                    if !r.can_backfill(self.current_time, duration) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Get reservation for a specific partition
    pub fn partition_reservation(&self, partition_id: &str) -> Option<&Reservation> {
        self.items.iter().find(|r| r.partition_id == partition_id)
    }

    /// Get all reservations for a node
    pub fn node_reservations(&self, node_id: &str) -> Vec<&Reservation> {
        self.by_node.get(node_id)
            .map(|indices| {
                indices.iter()
                    .filter_map(|&idx| self.items.get(idx))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Advance current time and cleanup expired reservations
    pub fn advance_time(&mut self, delta: u32) {
        self.current_time = self.current_time.saturating_add(delta);
        self.cleanup_expired();
    }

    /// Get current time
    pub fn current_time(&self) -> u32 {
        self.current_time
    }
}

/// A backfill window - opportunity to run short tasks
#[derive(Debug, Clone)]
pub struct BackfillWindow {
    pub node_id: String,
    pub start_time: u32,
    pub end_time: u32,
    pub duration: u32,
    pub max_priority: ReservationPriority,
}

impl BackfillWindow {
    /// Check if a task fits in this window
    pub fn can_fit(&self, duration: u32, priority: ReservationPriority) -> bool {
        duration <= self.duration && priority <= self.max_priority
    }

    /// Get the available time range
    pub fn range(&self) -> Range<u32> {
        self.start_time..self.end_time
    }
}

/// Integration with scheduler policy
pub trait ReservationAwarePolicy {
    /// Select a node considering reservations
    fn select_with_reservations(
        &self,
        partition_id: &str,
        node_ids: &[String],
        duration: u32,
        preferred_time: u32,
        reservations: &ReservationTable,
    ) -> Option<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reservation_creation() {
        let r = Reservation::new("p1".to_string(), "n1".to_string(), 100, 200);
        assert_eq!(r.duration(), 100);
        assert!(r.is_active_at(150));
        assert!(!r.is_active_at(50));
        assert!(!r.is_active_at(200));
    }

    #[test]
    fn test_reservation_overlap() {
        let r = Reservation::new("p1".to_string(), "n1".to_string(), 100, 200);
        assert!(r.overlaps(150, 10));
        assert!(!r.overlaps(50, 40));
        assert!(!r.overlaps(200, 10));
    }

    #[test]
    fn test_backfill_check() {
        let r = Reservation::new("p1".to_string(), "n1".to_string(), 100, 200);

        // Short task before reservation starts - should allow
        assert!(r.can_backfill(90, 5));

        // Task that would overlap - should not allow
        assert!(!r.can_backfill(95, 10));

        // Task after reservation - should allow
        assert!(r.can_backfill(200, 10));
    }

    #[test]
    fn test_reservation_table() {
        let mut table = ReservationTable::new().with_current_time(50);

        table.add(Reservation::new("p1".to_string(), "n1".to_string(), 100, 200));
        table.add(Reservation::new("p2".to_string(), "n1".to_string(), 150, 250));

        assert_eq!(table.items.len(), 2);

        // Check scheduling
        assert!(table.can_schedule_at("n1", 50, 40));
        assert!(!table.can_schedule_at("n1", 90, 20));
        assert!(table.can_schedule_at("n1", 250, 10));
    }

    #[test]
    fn test_earliest_start_time() {
        let mut table = ReservationTable::new().with_current_time(0);

        table.add(Reservation::new("p1".to_string(), "n1".to_string(), 100, 200));

        let start = table.earliest_start_time("n1", 50, 50);
        assert_eq!(start, Some(50)); // Before reservation

        let start = table.earliest_start_time("n1", 80, 50);
        assert_eq!(start, Some(200)); // After reservation
    }

    #[test]
    fn test_backfill_windows() {
        let mut table = ReservationTable::new().with_current_time(0);

        table.add(Reservation::new("p1".to_string(), "n1".to_string(), 100, 200));

        let windows = table.find_backfill_windows("n1", 200);
        assert!(!windows.is_empty());

        // First window should be before reservation
        let first = &windows[0];
        assert_eq!(first.start_time, 0);
        assert!(first.end_time <= 100);
    }

    #[test]
    fn test_cleanup_expired() {
        let mut table = ReservationTable::new().with_current_time(200);

        table.add(Reservation::new("p1".to_string(), "n1".to_string(), 50, 100));
        table.add(Reservation::new("p2".to_string(), "n1".to_string(), 150, 250));

        table.cleanup_expired();

        assert_eq!(table.items.len(), 1);
        assert_eq!(table.items[0].partition_id, "p2");
    }
}
