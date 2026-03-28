# URP 调度系统能力说明

> **版本**：v0.8
> **日期**：2026-03-28
> **状态**：✅ 调度测试全部通过（7/8，1个测试发现 DAG 输入键绑定问题待修复）

---

## 概述

URP 的调度系统负责把一张 IRGraph（计算任务图）拆分成多个分区，并根据节点的标签、区域、容量和历史惯性，把每个分区绑定到最合适的算力节点上执行。执行完成后通过数据包路由把结果传递给下游节点，最终合并输出。

```
IRGraph
  │
  ├─ fuse_linear_blocks()      ← 融合相邻兼容块，减少跨节点通信
  │
  ├─ partition_graph()         ← 按 tag/zone/shape 切分成逻辑分区
  │
  ├─ bind_partitions()         ← MultifactorPolicy 把分区绑定到节点
  │      │
  │      └─ node_score()       ← 计算节点得分（tag匹配+区域+容量+惯性）
  │
  ├─ topo_order()              ← 拓扑排序，确定执行顺序
  │
  ├─ BlockExecutor::exec()     ← 执行每个块的 Opcode
  │
  ├─ route (local-ring / remote-packet)  ← 传递中间结果
  │
  └─ run_reducers()            ← 按 MergeMode 合并最终结果
```

---

## 节点模型

每个节点（`Node`）有以下属性决定调度行为：

| 属性 | 类型 | 说明 |
|------|------|------|
| `tags` | `Vec<String>` | 能力标签，如 `"cpu"`, `"gpu"` |
| `zone` | `String` | 区域标识，如 `"zone-a"`, `"zone-b"` |
| `host_id` | `String` | 宿主机标识，同 host 走本地环，不同 host 走远程包 |
| `compute_capacity` | `f32` | 算力容量，越大得分越高 |
| `bandwidth` | `f32` | 带宽，影响路由开销计算 |
| `inertia_keys` | `Vec<String>` | 已缓存的惯性键，命中时调度优先度大幅提升 |

支持的节点类型（`NodeType`）：`Cpu`, `Gpu`, `Qcu`, `Memory`, `Network`, `Rule`, `Structure`

---

## 调度评分模型

`MultifactorPolicy` 对每个节点计算综合得分：

```
score = 0

if node.has_tag(required_tag):
    score += 2.0
else:
    score = -1e9  ← 直接排除，不可调度

if node.zone == preferred_zone:
    score += 1.5

score += 0.1 * node.compute_capacity
score += 0.02 * node.bandwidth

if node.has_inertia_key(inertia_key):
    score += 3.0   ← 惯性加成，权重最高
```

**优先级**：惯性命中 (3.0) > 区域匹配 (1.5) > 容量 (0.1x) > 带宽 (0.02x)

---

## 路由开销模型

```
route_cost = 0

if src.host_id != dst.host_id:
    cost += 10.0   ← 跨机器，走远程包

if src.zone != dst.zone:
    cost += 3.0    ← 跨区域

cost += (100.0 / dst.bandwidth) * 0.1
```

同一 `host_id` 的两个节点使用 **LocalRingTunnel**（内存环形队列，无网络开销）。
不同 `host_id` 的节点使用 **RemotePacketLink**（TCP 网络传输）。

---

## Block Fusion（块融合）

当相邻两个块满足以下**全部条件**时自动融合为一个块：

1. `required_tag` 相同
2. `preferred_zone` 相同
3. `resource_shape` 相同
4. 两者之间有且仅有一条直接边

融合效果：
- 减少分区数量，降低跨节点通信次数
- 融合后的块 `estimated_duration` = 两者之和
- 融合后的块继承 `inertia_key`（优先取后块，后块为 None 则继承前块）

```
[a] --edge--> [b]   (同 tag/zone/shape)
        ↓ fuse
    [a+b]            (减少一次数据包传递)
```

---

## Graph Partitioning（图分区）

按顺序扫描图中的块，相邻块具有相同 `required_tag + resource_shape + preferred_zone` 则归入同一分区，否则开新分区。

示例（5个块）：

```
cpu1 (cpu, zone-a, small) ─┐
cpu2 (cpu, zone-a, small) ─┘─→ 分区 p0  → 绑定到 cpu 节点 zone-a

gpu1 (gpu, zone-a, small) ─┐
gpu2 (gpu, zone-a, small) ─┘─→ 分区 p1  → 绑定到 gpu 节点 zone-a

cpu3 (cpu, zone-b, small) ────→ 分区 p2  → 绑定到 cpu 节点 zone-b
```

---

## 已验证的调度能力

以下测试均通过（见 `tests/scheduling_test.rs`）：

### ✅ 1. 标签分发（Tag-based Dispatch）

不同 `required_tag` 的任务被路由到对应类型的节点：

```
cpu-task (tag=cpu) → cpu0
gpu-task (tag=gpu) → gpu0
```

### ✅ 2. 区域感知选择（Zone-aware Selection）

相同类型节点中，优先选择与 `preferred_zone` 匹配的节点：

```
task-a (prefers zone-a) → cpu-zone-a  (+1.5 区域分)
task-b (prefers zone-b) → cpu-zone-b  (+1.5 区域分)
```

### ✅ 3. 惯性亲和（Inertia Affinity）

已缓存过某个 `inertia_key` 的节点获得 +3.0 加分，会被优先重复调度（数据局部性）：

```
cpu0 (已缓存 model-weights-v1) → inference 任务 → cpu0  (+3.0 惯性分)
cpu1 (未缓存)                  →                → 未选中
```

**适用场景**：模型推理、热数据缓存、重复计算批次等需要数据局部性的场景。

### ✅ 4. 容量优先选择（Capacity-based Selection）

同区域同标签节点中，计算容量更高的节点得分更高：

```
cpu-strong (capacity=100) → task  (+10.0 容量分)
cpu-weak   (capacity=10)  →       (+1.0 容量分，未选中)
```

### ✅ 5. 本地 vs 远程路由（Local-ring vs Remote-packet）

```
同 host_id 节点间 → local-ring  (route_cost ≈ 0.1)
不同 host_id 节点间 → remote-packet (route_cost = 10.x)
```

### ✅ 6. 块融合（Block Fusion）

兼容对（同 tag/zone/shape + 直接边）自动融合：
```
[a] + [b] → [a+b]  (1 个块，减少 1 次通信)
```

不兼容的保持独立：
```
[x(small)] + [y(large)] + [z(small)] → 3 个块（shape 不同，不融合）
```

### ✅ 7. 图分区（Graph Partitioning）

```
cpu1+cpu2 → p0   (同 tag/zone/shape，融合后同分区)
gpu1+gpu2 → p1   (不同 tag，新分区)
cpu3      → p2   (不同 zone，新分区)
共 3 个分区
```

---

## 已知限制

### DAG 输入键绑定问题

`test_dag_dependency_chain` 发现一个 bug：

**现象**：DAG 中 `UAdd` 节点期望从 inbox 中读取键名为 `"a"` 和 `"b"` 的输入，但 `IREdge` 的 `input_key` 与 `IRBlock.inputs[]` 之间的绑定需要调用方手动保持一致。

**根因**：`BlockExecutor::exec` 的 `UAdd` 实现从 `block.inputs[0]` 和 `block.inputs[1]` 取键名，然后在 `ctx` map 里查找。但 `ctx` 的键由路由时的 `edge.input_key` 决定。两者必须手动对齐。

**当前状态**：已记录，待修复。涉及 `executor.rs` 的 `UAdd`/`UConcat` 输入绑定逻辑。

---

## 执行结果结构

`execute_graph()` 返回 `RuntimeResult`，包含：

| 字段 | 说明 |
|------|------|
| `block_binding` | 每个块 → 被调度到的节点 ID |
| `partition_binding` | 每个分区 → 节点 ID |
| `results` | 每个块的执行值、合并模式 |
| `packet_log` | 所有数据包路由记录（src/dst/route_type/cost） |
| `merged` | 按 MergeMode 合并后的最终结果 |
| `remote_sent_packets` | 通过远程链路发送的包数量 |

---

## MergeMode（合并语义）

| 值 | 含义 |
|----|------|
| `List` | 所有结果收集为列表 |
| `Sum` | 对 I64 结果求和 |
| `Concat` | 对字符串/混合结果拼接 |
| `ReduceMax` | 取最大值（保留） |

---

## 扩展接口

### 自定义调度策略

实现 `SchedulerPolicy` trait：

```rust
impl SchedulerPolicy for MyPolicy {
    fn select_partition_node(
        &self,
        required_tags: &HashSet<String>,
        preferred_zone: &str,
        inertia_key: Option<&str>,
        nodes: &HashMap<String, Node>,
    ) -> String {
        // 返回选中的 node_id
    }
}
```

### 新增 Opcode

在 `src/ir.rs` 的 `Opcode` 枚举中添加，并在 `src/executor.rs` 的 `BlockExecutor::exec` 中实现对应逻辑。

---

## 与 kdmapper 的关系

URP 调度系统目前运行在用户空间的虚拟节点上。结合 kdmapper，可以：

1. 通过 kdmapper 加载自定义内核驱动，向用户空间暴露硬件能力
2. 把该硬件封装为 URP `Node`，打上对应 `tag`（如 `"fpga"`, `"asic"`）
3. IRGraph 中有 `required_tag=fpga` 的块会被自动调度到该节点执行

从而实现**无官方驱动的自定义硬件接入 URP 调度体系**。
