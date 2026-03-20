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

**URX Runtime** 是围绕 **“万物皆为可调度的算力节点”** 这一核心思想构建的执行底座。  
它不是普通 batch 调度器，也不是单纯虚拟机管理器，而是朝着：

> **可计算宇宙执行图 runtime**

推进的中层架构。

---

## 3. 当前版本定位

本包主线代码以 **Rust v0.8** 为准。  
它已经具备：

- IRGraph
- block fusion
- graph partition
- partition-level binding
- local ring fast path
- remote packet path skeleton
- reducer trait
- scheduler policy trait
- topology-aware cost model
- reservation / backfill skeleton

所以它的状态不是“从零开始”，而是：

> **已经有一条可运行的架构主干，接下来该继续实化。**

---

## 4. 目录说明

### `src/ir.rs`
定义：

- `IRBlock`
- `IRGraph`
- `MergeMode`
- `Opcode`

这是执行语义的入口。

### `src/optimizer.rs`
定义：

- `fuse_linear_blocks`
- `partition_graph`

这是执行前图优化层。

### `src/partition.rs`
定义分区级绑定逻辑。  
当前是 **partition-level binding skeleton**。

### `src/policy.rs`
定义：

- `SchedulerPolicy`
- `MultifactorPolicy`

这是策略层入口。后续可以继续扩：

- topology-aware policy
- inertia-first policy
- reservation-aware policy
- cost-balanced policy

### `src/cost.rs`
定义：

- `node_score`
- `route_cost`

这是成本模型起点。

### `src/reservation.rs`
定义 reservation / backfill 骨架。  
现在还是 skeleton，但接口位已经留好。

### `src/packet.rs`
定义：

- `URPPacket`
- `PacketHeader`
- `PayloadCodec`

这是 packet-first / zero-copy style 的关键。

### `src/ring.rs`
本地 `LocalRingTunnel`。  
这是 **local fast path** 的核心。

### `src/remote.rs`
`RemotePacketLink` skeleton。  
目前只是最小抽象，后续应继续实化。

### `src/reducer.rs`
Reducer trait + built-in reducers。  
这是 merge 语义可扩展层。

### `src/runtime.rs`
当前最重要文件之一。  
它把：

- optimize
- partition
- bind
- execute
- local/remote route
- reducer merge

串成端到端链路。

### `src/main.rs`
最小 demo。  
用于帮助接手者快速读懂主链。

---

## 5. 设计原则

### 原则 1：URX 不是“更大的云超算控制器”
不要把它降级理解成：

- 异构机器调度器
- 云资源大杂烩
- batch job orchestrator

这些只是局部影子。  
URX 的主线是：

> **把执行对象从“机器”提升为“节点宇宙”。**

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
不要把后续实现退回成“直接 job 分发器”。

### 原则 4：packet-first / local-ring-first
本地节点之间优先走：

- packet
- header/payload split
- local ring fast path

不要回退到“大量对象来回复制”的老路。

### 原则 5：merge 必须可扩展
目前已有 reducer trait。  
后续所有新 merge 逻辑尽量都挂在 reducer 扩展点下，不要在 runtime 里硬编码。

---

## 6. 当前哪些地方是骨架，哪些地方是方向已定

### 已定方向
这些不建议推翻：

- IRGraph 作为执行图入口
- fusion -> partition -> binding -> execute 主链
- packet-first runtime
- local ring fast path
- reducer trait
- scheduler policy trait
- topology-aware cost model
- reservation/backfill 留口

### 仍属骨架
这些可以继续实化：

- `RemotePacketLink`
- 更真实的 partition DAG scheduling
- 真 async execution lanes
- reservation/backfill 实际逻辑
- richer node model
- real zero-copy shared memory path
- 真正的跨 host transport

---

## 7. 最值得优先继续的顺序

### 第一优先级
把 `remote.rs` 从 skeleton 推到可用接口：

- request / response framing
- delivery semantics
- retry / ack skeleton
- remote cost hooks

### 第二优先级
把 `runtime.rs` 推成 **partition DAG scheduling**：

- 分区内局部顺序
- 分区间 DAG 调度
- async lane 抽象

### 第三优先级
把 reservation/backfill 从“占位”推进成实际策略：

- earliest reserved start
- can_backfill()
- policy integration

### 第四优先级
把 zero-copy 再前进一步：

- payload view 更明确
- shared memory path skeleton
- buffer reuse / inertia reuse 结合

---

## 8. 不要优先做什么

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

## 9. 建议 Claude 的接手方式

最稳的接手方式是：

1. 先通读 `README.md`
2. 再看 `src/main.rs`
3. 再看 `src/runtime.rs`
4. 然后看：
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

## 10. 可直接复制给 Claude 的一句话任务定义

> 你正在接手一个名为 URX Runtime 的 Rust 架构骨架。请不要把它理解成普通集群调度器。它的主线是：IRGraph -> graph optimization -> partition -> policy-based binding -> local/remote packet route -> reducer merge。当前版本是 v0.8，已具备 fusion、partition、policy、cost、reservation skeleton、packet-first runtime 和 local-ring-first fast path。接手目标是继续把 remote path、partition DAG scheduling 和 reservation/backfill 实化。

---

## 11. 最后一句

这个包的正确接手姿势不是“把它改成更常见的工程”，而是：

> **守住 URX 的世界观，再把运行时一层一层做实。**

一句话总结：

> **不要把 URX 降级成工具；要把它继续推进成执行宇宙的中层语义与运行时。**
