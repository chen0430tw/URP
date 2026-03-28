//! URX Runtime v0.8
//!
//! A runtime for the Universal Reconstructive eXtensions (URX) architecture.
//! This runtime provides:
//!
//! - IRGraph-based task representation
//! - Block fusion and graph partitioning
//! - Policy-based scheduling with topology-aware cost modeling
//! - Packet-first execution with local ring fast path
//! - Remote packet routing for cross-node execution
//! - Flexible result merging via reducer traits
//!
//! # Quick Start
//!
//! ```rust
//! use urx_runtime_v08::*;
//!
//! #[tokio::main]
//! async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
//!     // Create an IR graph
//!     let mut graph = IRGraph::new();
//!
//!     // Add blocks to the graph
//!     let block1 = IRBlock::new("block1", Opcode::UConstI64(42));
//!     graph.blocks.push(block1);
//!
//!     // Create nodes
//!     let node1 = Node::new("node1", NodeType::Cpu, 100.0);
//!
//!     // Create runtime
//!     let mut runtime = URXRuntime::new(vec![node1], MultifactorPolicy::new());
//!
//!     // Execute
//!     let result = runtime.execute_graph(&graph).await;
//!
//!     println!("Result: {:?}", result);
//!     Ok(())
//! }
//! ```

pub mod cost;
pub mod executor;
pub mod ir;
pub mod node;
pub mod optimizer;
pub mod packet;
pub mod partition;
pub mod policy;
pub mod reducer;
pub mod remote;
pub mod reservation;
pub mod ring;
pub mod runtime;
pub mod scheduler;
pub mod shared_memory;

// KDMapper FFI module (optional, requires "kdmapper" feature)
#[cfg(feature = "kdmapper")]
pub mod kdmapper_ffi;

// Re-export kdmapper types when feature is enabled
#[cfg(feature = "kdmapper")]
pub use kdmapper_ffi::{
    DriverMappingConfig, DriverMappingResult, KDMapperError, KDMapperExecutor,
    KernelModuleInfo, MemoryOperationResult, PoolType, Result,
};

// Re-export commonly used types
pub use cost::{node_score, route_cost};
pub use executor::{BlockExecutor, CpuExecutor, eval_opcode, ExecutorRegistry, HardwareExecutor, ThreadPoolExecutor};
pub use ir::{IRBlock, IREdge, IRGraph, MergeMode, Opcode};
pub use node::{Node, NodeType};
pub use optimizer::{fuse_linear_blocks, partition_graph};
pub use packet::{PayloadCodec, PayloadValue, URPPacket};
pub use partition::bind_partitions;
pub use policy::{MultifactorPolicy, SchedulerPolicy};
pub use reducer::{Reducer};
pub use remote::{LinkConfig, RemotePacketLink};
pub use reservation::{
    BackfillWindow, Reservation, ReservationPriority, ReservationTable,
    ReservationAwarePolicy,
};
pub use ring::LocalRingTunnel;
pub use runtime::{RuntimeResult, URXRuntime};
pub use scheduler::{AsyncLane, Partition, PartitionDAGScheduler};
pub use shared_memory::{
    BufferPool, CacheStats, InertiaBufferCache, PayloadView, PoolStats,
    SharedMemoryRegion, ZeroCopyContext,
};

/// Current version of the URX runtime
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Description of the URX runtime
pub const DESCRIPTION: &str = concat!(
    "URX Runtime v",
    env!("CARGO_PKG_VERSION"),
    " - scheduler policy, topology cost, reservation/backfill"
);
