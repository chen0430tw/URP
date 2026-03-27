# URX Runtime v0.8

通用拟构扩展指令体系（URX）的 Rust Runtime 实现。

## 快速开始

### 安装依赖

确保已安装 Rust 工具链：

```bash
# 检查 Rust 版本
rustc --version
cargo --version
```

### 构建项目

```bash
# 克隆项目
cd URP

# 构建项目
cargo build

# 运行示例
cargo run
```

### 运行测试

```bash
# 运行所有测试
cargo test

# 运行测试并显示输出
cargo test -- --nocapture

# 运行特定测试
cargo test test_simple_graph_execution
```

## 项目概述

URX Runtime 是一个面向”可计算宇宙”的执行底座，核心理念是：

> **万物皆为可调度的算力节点**

### 核心特性

- **IRGraph 执行模型** - 任务先被结构化为图，再执行
- **Block Fusion** - 自动融合相邻的可合并块
- **Graph Partition** - 按资源需求分区
- **Policy-based Scheduling** - 可替换的调度策略
- **Topology-aware Cost Model** - 考虑区域、带宽、惯性的成本模型
- **Packet-first Execution** - 数据包优先的执行语义
- **Local Ring Fast Path** - 本地节点快速通道
- **Remote Packet Routing** - 跨节点数据包路由
- **Flexible Merge Semantics** - 可扩展的结果合并

### 执行链路

```
IRGraph → fusion → partition → policy-based binding
→ execute → local/remote packet route → reducer merge
```

## 主要模块

| 模块 | 文件 | 功能 |
|------|------|------|
| Runtime | `src/runtime.rs` | 端到端执行引擎 |
| Policy | `src/policy.rs` | 调度策略接口和实现 |
| Cost | `src/cost.rs` | 拓扑感知成本模型 |
| Reservation | `src/reservation.rs` | 资源预留和回填 |
| Optimizer | `src/optimizer.rs` | 图优化（融合、分区） |
| Packet | `src/packet.rs` | 数据包结构和编解码 |
| Ring | `src/ring.rs` | 本地环形通道 |
| Remote | `src/remote.rs` | 远程网络通信 |
| Reducer | `src/reducer.rs` | 结果合并语义 |
| IR | `src/ir.rs` | 中间表示定义 |
| Node | `src/node.rs` | 节点数据模型 |
| Partition | `src/partition.rs` | 分区绑定逻辑 |
| Executor | `src/executor.rs` | 块执行器 |

## 使用示例

### 基础用法

```rust
use urx_runtime::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建 IR 图
    let mut graph = IRGraph::new();

    // 添加块
    let block1 = IRBlock::new(“const1”, Opcode::UConstI64(42));
    let block2 = IRBlock::new(“const2”, Opcode::UConstI64(58));
    let block3 = IRBlock::new(“add”, Opcode::UAdd);

    graph.add_block(block1);
    graph.add_block(block2);
    graph.add_block(block3);

    // 添加依赖边
    graph.add_edge(“const1”, “add”);
    graph.add_edge(“const2”, “add”);

    // 创建节点
    let cpu_node = Node::new(“cpu1”, NodeType::Cpu, 100);

    // 创建运行时
    let runtime = URPRuntime::new();
    let policy = MultifactorPolicy::new();

    // 执行
    let result = runtime.execute_with_policy(graph, vec![cpu_node], &policy).await?;

    println!(“Result: {:?}”, result);
    Ok(())
}
```

### 自定义调度策略

```rust
use urx_runtime::*;

struct MyPolicy;

impl SchedulerPolicy for MyPolicy {
    fn select_node(
        &self,
        block: &IRBlock,
        nodes: &[Node],
        cost_model: &HashMap<String, f64>,
    ) -> Option<Node> {
        // 自定义节点选择逻辑
        nodes.first().cloned()
    }
}

// 使用自定义策略
let runtime = URPRuntime::new();
let policy = MyPolicy;
let result = runtime.execute_with_policy(graph, nodes, &policy).await?;
```

### 远程执行

```rust
use urx_runtime::*;

// 创建远程链路
let mut remote_link = RemotePacketLink::new();

// 发送数据包到远程节点
let packet = URPPacket::build(
    Opcode::UConstI64(123),
    “my_block”.to_string(),
    MergeMode::List,
);

let response = remote_link.send(“127.0.0.1:8080”, packet).await?;
```

## 开发指南

### 代码结构

```
URP/
├── src/
│   ├── main.rs              # 项目入口和示例
│   ├── lib.rs               # 库入口，导出公共 API
│   ├── runtime.rs           # 核心运行时
│   ├── policy.rs            # 调度策略
│   ├── cost.rs              # 成本模型
│   ├── reservation.rs       # 资源预留
│   ├── optimizer.rs         # 图优化
│   ├── packet.rs            # 数据包
│   ├── ring.rs              # 本地通道
│   ├── remote.rs            # 远程通信
│   ├── reducer.rs           # 结果合并
│   ├── ir.rs                # 中间表示
│   ├── node.rs              # 节点模型
│   ├── partition.rs         # 分区绑定
│   ├── executor.rs          # 块执行器
│   ├── packet_test.rs       # 数据包测试
│   └── runtime_test.rs      # 运行时测试
├── tests/
│   └── integration_test.rs  # 集成测试
├── Cargo.toml               # 项目配置
└── README.md                # 本文档
```

### 添加新功能

1. **新的 Opcode** - 在 `src/ir.rs` 中添加
2. **新的 MergeMode** - 在 `src/ir.rs` 和 `src/reducer.rs` 中添加
3. **新的 NodeType** - 在 `src/node.rs` 中添加
4. **新的调度策略** - 实现 `SchedulerPolicy` trait

### 运行测试

```bash
# 单元测试
cargo test --lib

# 集成测试
cargo test --test integration_test

# 带输出的测试
cargo test -- --nocapture

# 带日志的测试
RUST_LOG=debug cargo test
```

## 版本历史

### v0.8 (当前)
- ✅ Scheduler policy trait
- ✅ Topology-aware cost model
- ✅ Reservation / backfill skeleton
- ✅ 完整测试套件
- ✅ 真正的网络通信实现

### v0.7
- Remote packet path
- Partition-level binding
- Reducer trait

### v0.6
- Rust 版 fusion + partition
- Inertia-aware reuse

### v0.5
- Rust 版 IRBlock -> packet -> ring -> merge

## 许可证

MIT License
