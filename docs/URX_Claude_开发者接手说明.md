# URX Claude / 开发者接手说明

## 1. 这份包是什么

这是 **URX Runtime ClaudeReady 接手包**。  
目标不是展示概念，而是让协作者直接知道：

- 目前做到哪一版
- 哪些模块已经成型
- 下一步该从哪里继续
- 哪些地方只是骨架，哪些地方已经是明确方向
- 哪些原则不能改丢

---

## 2. 项目一句话

**URX Runtime** 是围绕 **"万物皆为可调度的算力节点"** 这一核心思想构建的执行底座。  
它不是普通 batch 调度器，也不是单纯虚拟机管理器，而是朝着：

> **可计算宇宙执行图 runtime**

推进的中层架构。

---

## 3. 当前版本定位

本包主线代码当前版本为 **v1.1**。  
它已经具备：

- IRGraph + JSON 序列化（serde 双向，文件 I/O）
- block fusion
- graph partition
- partition-level binding
- **PartitionDAGScheduler 完整集成**（DAG 拓扑调度 + AsyncLane 并行执行分区）
- local ring fast path
- remote packet path skeleton
- reducer trait
- scheduler policy trait + ReservationAwarePolicy 示例
- topology-aware cost model
- reservation / backfill（已有真实策略演示）
- ZeroCopyContext（SharedMemoryRegion / BufferPool / InertiaBufferCache）
- JIT 编译器（IRGraph → WGSL，CPU 路径）
- USB 二进制协议层（帧编解码、CRC8）
- 真实工作负载图执行（FFT / Transformer / ResNet）

所以它的状态不是"从零开始"，而是：

> **已经有一条可运行的完整执行链，且主干接口已经做实。**

---

## 4. 目录说明

### `src/ir.rs`
定义：

- `IRBlock`
- `IRGraph`
- `MergeMode`
- `Opcode`

所有类型都已添加 `serde::{Serialize, Deserialize}`，支持 JSON 往返。  
`IRGraph` 提供 `from_json` / `to_json` / `load_json` / `save_json`。

### `src/optimizer.rs`
定义：

- `fuse_linear_blocks`
- `partition_graph`

这是执行前图优化层。

### `src/partition.rs`
定义分区级绑定逻辑。

### `src/policy.rs`
定义：

- `SchedulerPolicy`
- `MultifactorPolicy`

这是策略层入口。后续可以继续扩：

- topology-aware policy
- inertia-first policy
- reservation-aware policy（已有 Demo K 示例）
- cost-balanced policy

### `src/cost.rs`
定义：

- `node_score`
- `route_cost`

这是成本模型起点。

### `src/reservation.rs`
定义 reservation / backfill。  
Demo K 演示了 `ReservationAwarePolicy` 的实现方式。

### `src/scheduler.rs`
定义：

- `Partition` — 含 `blocks`、`node_id`、`internal_order`
- `AsyncLane` — 信号量限速执行 lane，接口为 `Fn(Partition) -> Fut`
- `PartitionDAGScheduler` — Kahn 拓扑排序 + AsyncLane 分发

**重要**：`AsyncLane::execute_partition` 的闭包签名是 partition 级的：
```rust
F: Fn(Partition) -> Fut,
Fut: Future<Output = Vec<BlockExecutionResult>> + Send,
```
不要把它改回 block 级签名——这是 v1.1 解决的兼容性问题。

### `src/runtime.rs`
当前最重要文件之一。  
它把：

- optimize
- partition
- bind
- execute（通过 PartitionDAGScheduler）
- local/remote route
- reducer merge

串成端到端链路。

关键模式：所有可变执行状态包装为 `Arc<Mutex<>>`：
```rust
let shared_inbox  = Arc::new(Mutex::new(init_inbox));
let shared_log    = Arc::new(Mutex::new(Vec::<PacketLog>::new()));
let shared_rings  = Arc::new(Mutex::new(HashMap::new()));
let shared_remote = Arc::new(Mutex::new(RemotePacketLink::new()));
```

### `src/shared_memory.rs`
定义：

- `SharedMemoryRegion` — 带 reader 计数的共享字节区域
- `BufferPool` — acquire/release 缓冲区池，含统计
- `InertiaBufferCache` — 带 LRU 淘汰的热度感知缓存
- `ZeroCopyContext` — 上述三者的统一门面

### `src/main.rs`
Demo A–L。  
用于帮助接手者快速读懂主链，也是工作负载图加载的入口（Demo L）。

---

## 5. Opcode 命名规则（关键！）

> 代码生成 Agent 必须严格遵守此规则，否则 JSON 反序列化失败。

| 类型 | 正确命名 | 错误命名（不存在） |
|------|----------|------------------|
| 浮点常量 | `{"FConst": 3.14}` | `"UConstF64"` |
| 整型常量 | `{"UConstI64": 42}` | `"UConst"` |
| 浮点加 | `"FAdd"` | `"UAddF64"` |
| 浮点乘 | `"FMul"` | `"UMulF64"` |
| 浮点开方 | `"FSqrt"` | `"USqrtF64"` |
| 浮点比较 | `"FCmpLt"` | `"UCmpLtF64"` |
| i64→f64 | `"I64ToF64"` | `"UI64ToF64"` |
| f64→i64 | `"F64ToI64"` | `"UF64ToI64"` |

ReLU 没有原生 opcode，正确写法：
```
FCmpLt(FConst(0.0), x)  → cond（cond≠0 → input[1]，else → input[2]）
USelect(cond, x, FConst(0.0))
```

---

## 6. 设计原则

### 原则 1：URX 不是"更大的云超算控制器"
不要把它降级理解成：

- 异构机器调度器
- 云资源大杂烩
- batch job orchestrator

这些只是局部影子。  
URX 的主线是：

> **把执行对象从"机器"提升为"节点宇宙"。**

### 原则 2：节点中心，不是 CPU 中心
核心信条：

> **万物皆为可调度的算力节点。**

CPU/GPU/QCU/Memory/Rule/Structure 都只是节点种类。

### 原则 3：先图，后执行
任务先进入：

- IRBlock
- IRGraph
- graph optimization
- partition
- bind

然后才执行。  
不要把后续实现退回成"直接 job 分发器"。

### 原则 4：packet-first / local-ring-first
本地节点之间优先走：

- packet
- header/payload split
- local ring fast path

不要回退到"大量对象来回复制"的老路。

### 原则 5：merge 必须可扩展
目前已有 reducer trait。  
后续所有新 merge 逻辑尽量都挂在 reducer 扩展点下，不要在 runtime 里硬编码。

---

## 7. 当前哪些地方是骨架，哪些地方是方向已定

### 已定方向
这些不建议推翻：

- IRGraph 作为执行图入口（+ JSON 序列化）
- fusion -> partition -> binding -> execute 主链
- PartitionDAGScheduler + AsyncLane 的调度模型
- packet-first runtime
- local ring fast path
- reducer trait
- scheduler policy trait
- topology-aware cost model
- reservation/backfill 留口

### 仍属骨架（可继续实化）

- `RemotePacketLink`（目前最小抽象）
- reservation/backfill 更完整的策略树
- richer node model（算力计量、动态容量）
- 真正的跨 host transport
- GPU 路径下的 ZeroCopyContext 集成

---

## 8. 最值得优先继续的顺序

### 第一优先级
把 `RemotePacketLink` 从 skeleton 推到可用接口：

- request / response framing
- delivery semantics
- retry / ack skeleton
- remote cost hooks

### 第二优先级
把 reservation/backfill 推进成完整策略树：

- earliest reserved start
- can_backfill() 与 policy 集成
- 多策略组合

### 第三优先级
把 ZeroCopyContext 和 GPU 路径结合：

- payload view 更明确
- GPU buffer reuse / inertia reuse 结合
- 真正的 zero-copy shared memory path

---

## 9. 不要优先做什么

先不要把时间浪费在：

- UI
- 复杂数据库
- 账号系统
- 监控面板
- 过早的网络协议大全
- 花哨 benchmark 文案

当前最重要的是：

> **把 runtime 主链做扎实。**

---

## 10. 建议 Claude 的接手方式

最稳的接手方式是：

1. 先通读 `README.md`
2. 再看 `src/main.rs`（Demo A–L 是所有模块的活文档）
3. 再看 `src/runtime.rs`
4. 然后看：
   - `src/scheduler.rs`（理解 PartitionDAGScheduler + AsyncLane）
   - `src/policy.rs`
   - `src/cost.rs`
   - `src/reservation.rs`
   - `src/reducer.rs`
5. 最后才改：
   - `src/remote.rs`
   - `src/partition.rs`
   - `src/optimizer.rs`

原因很简单：  
`runtime.rs` 是当前主干，其它很多模块都是为它服务的。

---

## 11. Agent 工作站操作指南

### 场景
集群 Agent 生成计算图 JSON → 保存到工作站 → URX Demo L 加载执行。

### 工作站目录
```
C:\Users\asus\urp\
├── fft_n64_s6.json
├── fft_n128_s7.json
├── attn_h4_s32.json
└── resnet_8blk_c64.json
```

### JSON 最小示例
```json
{
  "graph_id": "my_graph",
  "blocks": [
    {
      "block_id": "a",
      "opcode": { "UConstI64": 21 },
      "inputs": [],
      "output": "a",
      "required_tag": "",
      "merge_mode": "List",
      "resource_shape": "",
      "preferred_zone": "",
      "inertia_key": null,
      "estimated_duration": 1
    },
    {
      "block_id": "b",
      "opcode": { "UConstI64": 21 },
      "inputs": [],
      "output": "b",
      "required_tag": "",
      "merge_mode": "List",
      "resource_shape": "",
      "preferred_zone": "",
      "inertia_key": null,
      "estimated_duration": 1
    },
    {
      "block_id": "sum",
      "opcode": "UAdd",
      "inputs": ["a", "b"],
      "output": "sum",
      "required_tag": "",
      "merge_mode": "List",
      "resource_shape": "",
      "preferred_zone": "",
      "inertia_key": null,
      "estimated_duration": 1
    }
  ],
  "edges": [
    { "src_block": "a", "dst_block": "sum", "output_key": "out", "input_key": "a" },
    { "src_block": "b", "dst_block": "sum", "output_key": "out", "input_key": "b" }
  ]
}
```

### 运行
```bash
# 在工作站 D:\URP 目录下：
cargo run 2>&1 | tail -50
# Demo L 会自动加载并执行 C:\Users\asus\urp\ 下的四个图
```

### 验证 JSON 格式
```bash
cargo test json_schema -- --nocapture
```

---

## 12. 可直接复制给 Claude 的一句话任务定义

> 你正在接手一个名为 URX Runtime 的 Rust 执行运行时，当前版本 v1.1。核心路径：IRGraph（含 JSON 序列化）→ graph optimization → partition → policy-based binding → PartitionDAGScheduler（DAG 拓扑调度 + AsyncLane 并行执行）→ local/remote packet route → reducer merge。所有可变执行状态通过 Arc<Mutex<>> 跨 async 闭包共享。接手目标是继续把 remote path 和 reservation/backfill 实化。Opcode 命名规则：U* = i64，F* = f64，`FConst(f64)` 是唯一浮点常量（不存在 UConstF64）。

---

## 13. 最后一句

这个包的正确接手姿势不是"把它改成更常见的工程"，而是：

> **守住 URX 的世界观，再把运行时一层一层做实。**

一句话总结：

> **不要把 URX 降级成工具；要把它继续推进成执行宇宙的中层语义与运行时。**
