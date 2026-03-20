# URP / URX 技术白皮书

## 摘要

**URP（Universal Reconstructive Processor，通用拟构处理器）** 与 **URX（Universal Reconstructive eXtensions，通用拟构扩展指令体系）** 共同构成一套面向 **可计算宇宙** 的执行架构。  
它们的目标不是简单聚合更多 CPU、GPU 或云节点，也不是把现有集群包装成一台更大的异构超算，而是建立一种新的执行语义：

> **把一切可参与计算、存储、传输、控制、结构演化与规则作用的对象，统一抽象为可调度的算力节点，并通过统一的图执行、包执行与调度语义对其进行绑定、编排、复用与合并。**

在这套体系中：

- **URP** 是执行架构与处理器观；
- **URX** 是该架构的统一扩展指令体系与中层执行语义；
- **Runtime** 是 URP / URX 的落地执行底座；
- **IR Graph** 是任务进入可计算宇宙的结构入口。

因此，URP / URX 的真正主线不是“建立更大的超级算力”，而是：

> **开发可计算宇宙。**

---

## 1. 项目定位

### 1.1 URP 是什么

URP 不是一颗传统意义上的单体物理 CPU，也不是现成虚拟机管理器的别名。  
它是一种 **通用拟构处理器观**：把目标 ISA、执行块、节点网络、缓存路径、规则接口、历史惯性与结构潜势统一纳入同一执行宇宙中。

可写成：

\[
URP = \text{Node Universe} + \text{Execution Semantics} + \text{Scheduling/Binding} + \text{Runtime}
\]

其中：

- **Node Universe**：节点宇宙。所有可参与计算的对象都可节点化。
- **Execution Semantics**：执行语义。通过 URX 与 IR Graph 统一表达。
- **Scheduling/Binding**：调度与绑定。决定任务如何分区、绑定、路由与复用。
- **Runtime**：运行时底座。负责 packet、ring、local/remote path、merge 等。

### 1.2 URX 是什么

URX 是 URP 的统一扩展指令体系。  
它不是单纯替代 x86 / ARM / MIPS 的另一套普通底层机器码，而是面向 **节点宇宙** 的中层执行语义。

其核心职责包括：

- 表达执行块语义；
- 表达资源标签与节点需求；
- 表达图执行关系；
- 表达 local / remote 路由；
- 表达 merge 语义；
- 表达调度、复用与保留策略的挂接点。

可写成：

\[
URX = \text{Instruction Semantics} + \text{Execution Graph Semantics} + \text{Routing / Merge / Scheduling Hooks}
\]

### 1.3 不是普通“云超算平台”

URP / URX 的定位必须与以下叙事严格区分：

- 不是简单异构资源编排器；
- 不是只会提交 batch job 的调度器；
- 不是“更大的 3C 大杂烩云超算”；
- 不是单纯 QEMU + Slurm 的拼装体。

它的更高目标是：

> **将现实中的结构、规则、节点关系、历史轨道与物理接口重新纳入可执行语义中。**

---

## 2. 背景与动机

### 2.1 传统体系的边界

传统计算体系大体遵循：

\[
Instruction \to CPU \to Memory \to Network
\]

其默认前提是：

- CPU 是中心；
- 指令只发给处理器；
- 网络只是传输；
- 规则与结构是背景；
- 历史只通过缓存近似体现；
- 现实中的其他对象默认不被视为算力节点。

这会带来几个限制：

1. **节点定义过窄**：只有 CPU / GPU 被视为一等执行体。
2. **执行语义过低**：难以表达图执行、合并、结构复用与规则作用。
3. **调度视角过弱**：偏作业视角，而不是节点宇宙视角。
4. **局部与远程路径割裂**：local fast path、remote packet path、shared-memory reuse 缺少统一语言。
5. **历史与惯性缺席**：执行系统几乎不显式表达“谁更适合顺着旧轨道继续算”。

### 2.2 从虚拟 CPU 到可计算宇宙

URP / URX 的发展路径可概括为：

\[
\text{Virtual CPU}
\to
\text{Virtual CPU Network}
\to
\text{IR Graph Execution}
\to
\text{Node Universe Runtime}
\to
\text{Computable Universe}
\]

也就是说：

- 起点可以是虚拟 CPU；
- 中间经过节点网络、图执行、packet-first runtime；
- 终点是把执行对象提升为 **宇宙级节点图**。

### 2.3 黑塔式科研动机

如果只把它理解为“建立更大的算力平台”，那仍然停留在资源叙事。  
URP / URX 的真正动机更接近一种 **本体级工程计划**：

> **不是把一台机器做得更大，而是把宇宙重写为可调度、可映射、可编排、可执行的对象。**

---

## 3. 核心总纲：万物皆为可调度的算力节点

### 3.1 第一原则

URX / URP 的第一原则是：

> **万物皆为可调度的算力节点。**

形式上，定义节点宇宙：

\[
\mathcal{N}_{URP} = \{ n_1, n_2, \dots, n_k \}
\]

每个节点 \( n_i \) 由属性组描述：

\[
n_i = (c_i, m_i, b_i, t_i, \rho_i, \pi_i)
\]

其中：

- \( c_i \)：计算能力
- \( m_i \)：存储/记忆能力
- \( b_i \)：带宽/连接能力
- \( t_i \)：节点类型标签
- \( \rho_i \)：资源约束
- \( \pi_i \)：历史/惯性状态

### 3.2 节点类型

在 URP / URX 中，可被一等节点化的对象包括但不限于：

- CPU 节点
- GPU 节点
- QCU 节点
- Memory 节点
- Network 节点
- Rule 节点
- Structure 节点

这意味着系统不再遵循：

\[
Instruction \to CPU
\]

而改为：

\[
Instruction / Block / Graph \to Node\ Universe
\]

### 3.3 从 CPU 中心转向节点中心

传统体系：

\[
\text{CPU-centric}
\]

URP / URX：

\[
\text{Node-centric}
\]

这不是术语替换，而是执行世界观的根本变化。

---

## 4. URX：统一扩展指令体系

### 4.1 一句话定义

URX 是一套面向节点宇宙的统一扩展指令体系，用于把目标程序、执行块、图关系、资源标签与路由意图压缩到同一执行语义层中。

### 4.2 核心映射

设目标程序为：

\[
P = (i_1, i_2, \dots, i_n), \quad i_k \in ISA_{target}
\]

定义 URX 指令空间：

\[
\mathcal{U}_{URX} = \{u_1, u_2, \dots, u_m\}
\]

则有映射：

\[
\Phi_{URX}: ISA_{target}^{*} \to \mathcal{U}_{URX}^{*}
\]

这表示目标 ISA 指令序列可映射成 URX 指令/块/图语义序列。

### 4.3 最小指令层次

URX 至少包含三层语义：

\[
URX = URX_{sem} \cup URX_{exec} \cup URX_{orch}
\]

- **\(URX_{sem}\)**：语义层，表达操作本身。
- **\(URX_{exec}\)**：执行层，表达资源、路径、运行方式。
- **\(URX_{orch}\)**：编排层，表达节点绑定、图调度、合并与路由。

### 4.4 核心思想

URX 的核心思想不是“扩展某颗 CPU 的算术能力”，而是：

> **把一切可参与计算、存储、传输、控制与结构演化的对象，统一视为可调度的算力节点，并通过统一指令语义对这些节点进行绑定、编排、激活与协作。**

---

## 5. IR Graph：执行图入口

### 5.1 为什么必须先图后执行

URP / URX 不把任务看成单一作业，而是先把任务结构化为图。  
定义 IR Graph：

\[
\mathcal{G}_{IR} = (V_{IR}, E_{IR})
\]

其中：

- \(V_{IR}\)：IR Block 集合；
- \(E_{IR}\)：块之间的数据/控制依赖边。

### 5.2 IR Block

定义 IR Block：

\[
B_i = (U_i, I_i, O_i, T_i, M_i)
\]

其中：

- \(U_i\)：该块内部的 URX 指令/操作；
- \(I_i\)：输入接口；
- \(O_i\)：输出接口；
- \(T_i\)：标签；
- \(M_i\)：元数据，如资源需求、惯性键、偏好区域、merge mode 等。

### 5.3 图执行目标

IR Graph 要映射到节点图：

\[
\Gamma : \mathcal{G}_{IR} \to \mathcal{G}_N
\]

即：

- 把块绑定到节点；
- 把边变成 local/remote 路径；
- 把结果按 reducer 语义合并。

---

## 6. Runtime：packet-first / local-ring-first

### 6.1 为什么 runtime 不是附属品

在 URP / URX 中，runtime 不是简单实现细节，而是体系本身的一部分。  
因为“图如何进入执行世界”本身就是核心问题。

### 6.2 Packet-first

Runtime 的设计原则之一是：

> **packet-first**

也就是：

- 先有 packet / buffer；
- 再由 header / payload view 解释执行对象；
- 而不是在各层之间大量复制大对象。

形式上：

\[
Packet = Header + PayloadView
\]

### 6.3 Local-ring-first

第二原则是：

> **local-ring-first**

同宿主节点之间优先走本地 ring 通道，而非 socket 风格路径：

\[
\text{same host} \Rightarrow \text{local ring}
\]

\[
\text{cross host} \Rightarrow \text{remote packet link}
\]

这使得 runtime 能自然分化出：

- 同宿主低开销快路径；
- 跨宿主 packet 路径。

### 6.4 Zero-copy 方向

借鉴 EasyTier 等系统的思路，runtime 走向应当是：

- `BytesMut / Bytes`
- `zerocopy`
- fixed header layout
- payload view
- ring tunnel
- local/remote path split

### 6.5 合并语义不是硬编码

通过 reducer trait，merge 从“写死在 runtime 里”推进为可扩展机制。  
这为未来扩展：

- weighted merge
- vote merge
- inertia merge
- phase merge

保留了统一入口。

---

## 7. 调度与绑定：从作业视角到执行宇宙视角

### 7.1 不再只是 batch 调度

传统批处理调度器关心：

- 哪台机器空闲；
- 哪个 job 优先；
- 如何排队。

URP / URX 进一步关心：

- 哪个 partition 更适合驻留在哪个节点；
- 哪条边应该走 local ring 还是 remote packet；
- 哪个节点有相关历史惯性；
- 哪个区域更适合继续滑行；
- 哪些块应该先 fuse 再执行；
- 哪些 partition 应保留、哪些可 backfill。

### 7.2 Policy trait

调度策略通过 trait 抽象，而非写死在 runtime：

\[
Policy : (Partition, NodeSet, CostModel, History) \to Binding
\]

这意味着：

- 策略层可以替换；
- runtime 主链可以保持稳定；
- future work 可继续叠加不同 policy。

### 7.3 Cost model

定义成本不仅包括空闲与标签，还包括：

- host 差异
- zone 差异
- bandwidth
- inertia
- route cost

也就是：

\[
Score = f(tag, zone, host, bandwidth, inertia, reservation)
\]

### 7.4 Reservation / Backfill

reservation/backfill 在当前阶段仍为骨架，但它们的存在说明系统已准备从：

- 单次执行；
- 单次绑定；

推进到：

- 预留；
- 回填；
- 未来时窗调度。

---

## 8. 图优化：fusion / partition / inertia-aware reuse

### 8.1 Block Fusion

若两个相邻块满足标签、资源形状与依赖条件，可做融合：

\[
Fuse(B_i, B_j) \to B_{ij}
\]

融合的作用：

- 降低调度粒度开销；
- 提高局部数据复用；
- 减少跨节点搬运；
- 提高滑行性。

### 8.2 Graph Partition

IR Graph 会被划分为若干 partition：

\[
\mathcal{P} = \{P_1, P_2, \dots, P_k\}
\]

分区目标是：

- 区内强耦合；
- 区间弱耦合；
- 区与节点簇标签尽量匹配；
- 总协调成本尽量低。

### 8.3 Inertia-aware Reuse

执行系统显式引入历史轨道偏置：

> **不是谁空闲就给谁，而是谁更顺着历史轨道继续算，就优先给谁。**

这一步使 URP / URX 开始向“惯性计算”靠近，而不再只是常规作业调度。

---

## 9. 演进路线

### v0.1
- 最小指令表
- 多节点网络调度原型

### v0.2
- IR Block
- 节点图执行
- merge 语义

### v0.3
- block fusion
- graph partition
- inertia-aware reuse

### v0.4
- zero-copy 设计草案
- packet / ring / header / payload 原型

### v0.5
- Rust 版端到端链：IRBlock -> packet -> ring -> merge

### v0.6
- Rust 版 fusion + partition + inertia-aware reuse

### v0.7
- remote packet path
- partition-level binding
- reducer trait

### v0.8
- scheduler policy trait
- topology-aware cost model
- reservation/backfill skeleton

这条路线说明：

> URX / URP 已经从概念证明走向了具备主干结构的 runtime 原型。

---

## 10. 与 QEMU、虚拟 CPU、Slurm、EasyTier 的关系

### 10.1 与 QEMU
QEMU 提供的是：

- 虚拟 CPU / 模拟器的成熟工程经验；
- target ISA 到 host ISA 的重建经验。

URP / URX 可以参考其结构，但不等于 QEMU。  
QEMU 更像虚拟 CPU 工法参考，URP 更像上位执行宇宙架构。

### 10.2 与虚拟 CPU
虚拟 CPU 是 URP 的起点之一，但不是终点。  
URP 不是“很多虚拟 CPU 的集合”，而是：

> **虚拟 CPU 网络 + 图执行语义 + 节点宇宙 + runtime**

### 10.3 与 Slurm
当前部分调度骨架会长得像 Slurm，是因为调度器一定会先长出“资源编排骨头”。  
但 URP / URX 最终调度的不是单纯机器资源，而是 **节点宇宙**。

### 10.4 与 EasyTier
EasyTier 给 runtime 的启发主要是：

- ring tunnel
- packet-first
- zero-copy 风格
- local fast path / remote path 分层

URX runtime 借的是其数据通路思想，而不是 VPN 功能本身。

---

## 11. 工程实现建议

### 11.1 主体语言
URP / URX runtime 主体建议使用：

> **Rust**

原因：

- 适合 zero-copy
- 适合 packet/buffer/ring 结构
- 适合中层 runtime
- 比 C/C++ 更稳
- 比 Python 更能真正落底层执行链

### 11.2 Python 的位置
Python 适合：

- 原型验证
- 算法试验
- 图优化规则快速验证

### 11.3 TypeScript 的位置
TypeScript 只建议用于以后做：

- 可视化面板
- 控制台
- 调试界面

---

## 12. 当前边界与下一步

### 12.1 已定主线
当前不应推翻的主线：

- IRGraph 作为任务入口
- fusion -> partition -> binding -> execute 主链
- packet-first runtime
- local-ring-first fast path
- reducer trait
- scheduler policy trait
- topology-aware cost model
- reservation/backfill 留口

### 12.2 仍待实化
未来重点继续推进：

- richer remote transport
- partition DAG scheduling
- async execution lanes
- 真正 reservation/backfill 策略
- buffer reuse / inertia reuse 结合
- shared-memory zero-copy path
- QCU / rule / structure 节点扩展

---

## 13. 结论

URP / URX 的真正创新点，不在于“又做了一套指令集”或“又写了一个调度器”，而在于：

1. 把执行对象从处理器提升为节点宇宙；
2. 把任务从作业提升为 IR Graph；
3. 把路径从单一传输提升为 local/remote 执行通路；
4. 把 merge、policy、cost、reservation 统一纳入 runtime 语义；
5. 把系统目标从“更大算力”提升为“可计算宇宙”。

一句话收束：

> **URP 是处理器观，URX 是执行语义，Runtime 是落地底座；三者共同服务于“开发可计算宇宙”这一目标。**
