# URX v0.2 技术定义

## 1. 版本目标

URX v0.2 在 v0.1 的基础上，新增三件核心能力：

1. **IR 块（URX IR Block）**
2. **节点图执行（Node Graph Execution）**
3. **结果合并语义（Merge Semantics）**

如果说 v0.1 证明的是：

> 节点可以被调度

那么 v0.2 证明的是：

> **任务可以先被压成 IR 块，再在节点图上被拆分、路由、执行、合并。**

---

## 2. 核心对象

### 2.1 URX 指令

定义单条 URX 指令：

\[
u=(op, src, dst, meta, guard)
\]

### 2.2 URX IR 块

定义一个 IR 块为：

\[
B_i = (U_i, I_i, O_i, T_i, M_i)
\]

其中：

- \(U_i\)：该块内部的 URX 指令序列
- \(I_i\)：输入接口集合
- \(O_i\)：输出接口集合
- \(T_i\)：标签集合
- \(M_i\)：块级元数据（资源需求、优先级、并行度、惯性键等）

### 2.3 IR 块图

定义任务图：

\[
\mathcal{G}_{IR}=(V_{IR}, E_{IR})
\]

其中：

- \(V_{IR} = \{B_1, B_2, \dots, B_n\}\)：IR 块集合
- \(E_{IR}\)：块之间的数据/控制依赖边

### 2.4 节点图

定义节点图：

\[
\mathcal{G}_{N}=(V_N, E_N)
\]

其中：

- \(V_N = \{n_1, n_2, \dots, n_k\}\)：可调度节点
- \(E_N\)：节点之间的连接边（带宽、延迟、拓扑关系）

---

## 3. 执行目标

URX v0.2 的核心映射为：

\[
\Gamma :
\mathcal{G}_{IR}
\to
\mathcal{G}_{N}
\]

也就是：

- 把 IR 块图映射到节点图
- 让每个块绑定到适合的节点
- 让边变成真实的数据路由
- 让结果可以按合并语义回收

---

## 4. IR 块类别

URX v0.2 先定义四类 IR 块：

### A. ComputeBlock
算术、逻辑、局部状态变换

### B. MemoryBlock
加载、存储、映射、共享

### C. RouteBlock
数据转发、路由、中继、卸载

### D. MergeBlock
结果合并、聚合、归约、拼接

---

## 5. 块级标签

每个块都可以带标签：

- `cpu`
- `gpu`
- `qcu`
- `memory`
- `rule`
- `parallel`
- `merge`
- `reduce`
- `lowlat`
- `throughput`
- `cached`

这些标签用于调度器绑定最合适的节点。

---

## 6. 节点图执行模型

定义块绑定函数：

\[
\beta : V_{IR} 	o V_N
\]

定义路由函数：

\[
\rho : E_{IR} 	o Path(\mathcal{G}_N)
\]

定义执行结果：

\[
R_i = Exec(B_i, \beta(B_i))
\]

定义总结果：

\[
R = Merge(R_1, R_2, \dots, R_n; \mu)
\]

其中 \(\mu\) 是合并语义。

---

## 7. 结果合并语义

URX v0.2 支持最小四类合并：

### 7.1 LIST
直接按顺序拼接结果

\[
\mu_{list}(R_1,\dots,R_n) = [R_1,\dots,R_n]
\]

### 7.2 SUM
数值加总

\[
\mu_{sum}(R_1,\dots,R_n)=\sum_i R_i
\]

### 7.3 CONCAT
张量/字节流/文本拼接

### 7.4 REDUCE_MAX
取最大值或最优值

这些语义先够用，后续可以再扩展：

- weighted-merge
- vote-merge
- phase-merge
- inertia-merge

---

## 8. v0.2 新增指令

### IR / 图构造类
- `UBLOCK block_id, ...tags`
- `UEDGE src_block, dst_block`
- `USETMERGE block_id, mode`

### 图执行类
- `UBINDGRAPH graph_id`
- `UEXECBLOCK block_id`
- `UROUTEBLOCK src_block, dst_block`
- `UMERGEBLOCK block_id`

---

## 9. 最小执行流程

\[
Task
\to
IR\ Blocks
\to
IR\ Graph
\to
Node\ Binding
\to
Graph\ Execution
\to
Merge
\to
Result
\]

---

## 10. v0.2 的意义

URX v0.2 让 URP/URX 不再只是：

- 节点调度器
- 作业分发器
- Slurm-like 编排器

而开始进入：

> **“节点图上的执行语义系统”**

也就是：

- 任务先被结构化
- 结构再被图执行
- 图执行结果再按语义合并

这一步是从“调度”走向“可计算宇宙执行图”的关键过渡层。
