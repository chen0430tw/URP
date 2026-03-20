# URX v0.3 技术定义

## 1. 版本目标

URX v0.3 在 v0.2 的基础上，继续推进三个关键能力：

1. **Block Fusion（块融合）**
2. **Graph Partition（图分区）**
3. **Inertia-aware Reuse（惯性感知复用）**

如果说：

- v0.1 证明了 **节点可调度**
- v0.2 证明了 **IR 块图可执行**

那么 v0.3 证明的是：

> **IR 块图不仅能执行，还能被重写、压缩、分区，并沿着历史轨道优先复用。**

---

## 2. Block Fusion

### 2.1 定义

给定两个相邻 IR 块 \(B_i, B_j\)，若满足：

1. 数据依赖简单且单向
2. 标签兼容
3. 合并后资源需求不越界
4. 合并不会破坏语义

则允许做块融合：

\[
Fuse(B_i, B_j) \to B_{ij}
\]

### 2.2 融合收益

定义融合收益函数：

\[
\Delta_F(B_i,B_j)
=
\alpha \cdot \mathrm{DataLocality}
+
\beta \cdot \mathrm{TagAffinity}
+
\gamma \cdot \mathrm{LaunchReduction}
-
\mu \cdot \mathrm{ResourcePenalty}
\]

当：

\[
\Delta_F(B_i,B_j) > 0
\]

则执行融合。

### 2.3 融合意义

块融合的本质是：

- 降低调度粒度开销
- 提高局部数据复用
- 减少节点间数据搬运
- 提高路径滑行性

---

## 3. Graph Partition

### 3.1 定义

给定 IR 图：

\[
\mathcal{G}_{IR}=(V_{IR},E_{IR})
\]

目标是把它划分成若干分区：

\[
\mathcal{P} = \{P_1, P_2, \dots, P_k\}
\]

使得：

- 分区内连接尽量强
- 分区间连接尽量弱
- 每个分区尽量匹配某类节点簇
- 总协调开销最小

### 3.2 目标函数

\[
\min
\Big(
\mathcal{L}_{cut}
+
\lambda_1 \mathcal{L}_{imbalance}
+
\lambda_2 \mathcal{L}_{mismatch}
\Big)
\]

其中：

- \(\mathcal{L}_{cut}\)：跨分区边代价
- \(\mathcal{L}_{imbalance}\)：分区负载不均衡代价
- \(\mathcal{L}_{mismatch}\)：分区与节点簇标签不匹配代价

### 3.3 分区意义

图分区让 URX 从“逐块调度”升级成“按子图调度”，这会带来：

- 更稳定的节点绑定
- 更低的跨区通信成本
- 更接近虚拟超算级的局部群执行

---

## 4. Inertia-aware Reuse

### 4.1 定义

对每个块或分区定义惯性键：

\[
\kappa(B_i) = key_i
\]

节点或节点簇维护历史复用集合：

\[
\mathcal{H}(n) = \{key_1,key_2,\dots\}
\]

若当前块/分区的惯性键与某节点历史轨道匹配，则给予复用增益：

\[
ReuseGain(B_i,n)
=
\mathbf{1}\{\kappa(B_i) \in \mathcal{H}(n)\}
\cdot w_r
\]

### 4.2 调度影响

节点得分更新为：

\[
Score(B_i,n)
=
Score_0(B_i,n)
+
ReuseGain(B_i,n)
\]

### 4.3 意义

这一步是 URX 开始逼近“惯性计算”概念的关键：

- 不是谁空闲就给谁
- 而是谁更顺着历史轨道继续算，就优先给谁

---

## 5. v0.3 新执行流程

\[
Task
\to
IR\ Graph
\to
Block\ Fusion
\to
Graph\ Partition
\to
Node\ Cluster\ Binding
\to
Inertia-aware\ Scheduling
\to
Execution
\to
Merge
\]

---

## 6. v0.3 新增语义

### 6.1 新元数据

每个块新增：

- `fusion_group`
- `partition_hint`
- `inertia_key`
- `resource_shape`

### 6.2 新操作意图

- `UFUSE`
- `UPARTITION`
- `UREUSEPATH`
- `UBINDPART`
- `UMERGEPART`

这些不一定要立刻变成显式用户指令，但要进入引擎内部语义。

---

## 7. 与前两版的关系

### v0.1
节点调度原型

### v0.2
IR 块图执行 + 合并语义

### v0.3
IR 图优化重写 + 分区绑定 + 惯性复用

也就是说，v0.3 标志着 URX 从：

> “能调度、能执行”

进入：

> “能先优化图，再执行图”

---

## 8. 版本意义

URX v0.3 的意义不在于“再加几个 opcode”，而在于：

> **把执行前优化正式纳入 URX 体系。**

从这一版开始，URX 不只是执行语言，也是：

- 图重写语言
- 分区语言
- 轨道复用语言

这已经明显比普通作业调度器更接近“可计算宇宙执行图”的方向。
