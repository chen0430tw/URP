use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Reservation {
    pub partition_id: String,
    pub node_id: String,
    pub start_time: u32,
    pub end_time: u32,
}

#[derive(Debug, Default)]
pub struct ReservationTable {
    pub items: Vec<Reservation>,
}

impl ReservationTable {
    pub fn add(&mut self, r: Reservation) {
        self.items.push(r);
    }

    pub fn earliest_reserved_start(&self) -> Option<u32> {
        self.items.iter().map(|r| r.start_time).min()
    }

    pub fn can_backfill(&self, now: u32, duration: u32) -> bool {
        match self.earliest_reserved_start() {
            Some(t) => now + duration <= t,
            None => true,
        }
    }

    pub fn partition_reserved_on(&self, partition_id: &str) -> Option<&Reservation> {
        self.items.iter().find(|r| r.partition_id == partition_id)
    }
}
