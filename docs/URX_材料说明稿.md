# URX 材料说明稿（更新版）

## URX 是什么

**URX（Universal Reconstructive eXtensions，通用拟构扩展指令体系）** 是 URP 的统一扩展指令层。  
它不是传统意义上只服务于单颗 CPU 的机器指令集，也不是简单模仿 x86、ARM、MIPS 的另一套低层 ISA。  
URX 的真正定位是：

> **把执行语义从“处理器中心”提升为“节点宇宙中心”。**

在 URX 的视角里，CPU、GPU、QCU、Memory、Network、Rule、Structure 都可以进入同一执行世界，并被统一表达、统一绑定、统一调度、统一合并。

---

## URX 的核心主张

URX 的第一原则可以压成一句话：

> **万物皆为可调度的算力节点。**

传统体系默认：

\[
Instruction \to CPU
\]

而 URX 的体系改写为：

\[
Instruction / Block / Graph \to Node\ Universe
\]

也就是说：

- 指令不再只发给某颗处理器；
- 块不再只是某段代码；
- 图不再只是编译器内部结构；
- 整个执行世界被重新理解为一个由节点构成的可调度宇宙。

---

## URX 为什么不是普通指令集

普通指令集通常解决的是：

- 如何描述算术逻辑操作；
- 如何描述寄存器与内存访问；
- 如何描述控制流跳转。

而 URX 除了这些，更关心：

- 如何表达执行块；
- 如何表达节点绑定；
- 如何表达 local / remote 路径；
- 如何表达 packet 级传递；
- 如何表达 merge 语义；
- 如何表达 graph execution；
- 如何表达调度、复用与保留策略的挂接点。

所以 URX 更准确地说，是一种：

> **统一扩展指令体系 + 图执行语义层 + 调度接口层**

---

## URX 的结构层次

URX 最好理解成三层结构：

\[
URX = URX_{sem} \cup URX_{exec} \cup URX_{orch}
\]

### 1. 语义层
负责表达：

- 操作本身
- 算术逻辑
- 控制语义
- 基础状态转移

### 2. 执行层
负责表达：

- block
- packet
- payload
- local / remote route
- merge mode
- reducer hook

### 3. 编排层
负责表达：

- partition
- binding
- scheduling policy
- reservation
- backfill
- inertia-aware reuse

这意味着 URX 不是只处理“怎么算”，还处理：

> **谁来算、在哪算、怎么传、怎么合。**

---

## URX 指令类别总表

前面已经正式定下来的 URX 指令类别集合为：

\[
\mathcal{C}_{URX}
=
\{
\mathcal{A},
\mathcal{M},
\mathcal{B},
\mathcal{R},
\mathcal{N},
\mathcal{S},
\mathcal{Q}
\}
\]

分别表示：

- \(\mathcal{A}\)：算术逻辑类
- \(\mathcal{M}\)：内存与映射类
- \(\mathcal{B}\)：分支控制类
- \(\mathcal{R}\)：资源提示类
- \(\mathcal{N}\)：网络编排类
- \(\mathcal{S}\)：结构与调度类
- \(\mathcal{Q}\)：特殊接口类

这七类共同组成 URX 的主干指令宇宙。

---

## URX v0.1 最小指令表

### A. 语义算子类 \(\mathcal{A}\)

用于基础算术逻辑语义：

- `UADD dst, a, b`
- `USUB dst, a, b`
- `UMUL dst, a, b`
- `UAND dst, a, b`
- `UXOR dst, a, b`

作用对象是：

- 基础寄存器态
- 局部 block 状态
- 轻量算术/逻辑块

---

### B. 内存/映射类 \(\mathcal{M}\)

用于数据装载、存储与映射：

- `ULOAD dst, addr`
- `USTORE addr, src`
- `UMAP region, tag`
- `USHARE region, node`

作用对象是：

- memory region
- block 输入输出视图
- 节点间共享区域

---

### C. 控制流类 \(\mathcal{B}\)

用于表达分支与过程控制：

- `UJMP label`
- `UBR cond, label_true, label_false`
- `UCALL target`
- `URET`

作用对象是：

- block 内局部控制流
- 基础执行跳转语义

---

### D. 资源提示类 \(\mathcal{R}\)

这一类在前期更像“语义挂接位”，用于给执行系统附加提示。典型形式包括：

- `UHINT_CACHE`
- `UHINT_LOCAL`
- `UHINT_VECTOR`
- `UHINT_PERSIST`
- `UHINT_INERTIA`

它们不是普通算术指令，而是告诉 runtime：

- 这块更适合 cache
- 这块更适合本地执行
- 这块更适合向量化
- 这块应尽量保留
- 这块更适合惯性复用

---

### E. 网络编排类 \(\mathcal{N}\)

用于节点网络级的分发与协作：

- `USPAWN node_type, node_id, ...tags`
- `UBIND task_id, node_id`
- `UROUTE src, dst, route_tag`
- `UJOIN group_id`
- `UOFFLOAD task_id, target_tag`

这类指令是 URX 和普通 ISA 最大的区别之一，因为它们作用的不是单颗 ALU，而是：

> **节点网络与任务路径。**

---

### F. 结构与调度类 \(\mathcal{S}\)

用于贴标签、绑定结构与调度策略：

- `UTAG object, ...tags`
- `UREUSE key`
- `USCHEDULE policy`

以及后续扩展里会进入这一层的：

- `UPIN`
- `UTRACE`
- `UCLASSIFY`

它们负责：

- 给对象贴标签
- 绑定历史轨道
- 提示调度策略
- 控制结构复用

---

### G. 特殊接口类 \(\mathcal{Q}\)

这类预留给未来特殊执行体或物理接口。前面已经定过的方向包括：

- `UQCALL`
- `UQC_SYNC`
- `UPHASE`
- `UCOHERE`

它们面向：

- QCU
- 相干单元
- 相位接口
- 特殊规则或结构接口

这一类当前仍属预留接口层，但已经是 URX 体系的一部分。

---

## URX v0.2 图执行指令

随着 URX 从“节点调度”推进到“IR 图执行”，后续又加入了一批图执行语义：

- `UBLOCK block_id, ...tags`
- `UEDGE src_block, dst_block`
- `USETMERGE block_id, mode`
- `UBINDGRAPH graph_id`
- `UEXECBLOCK block_id`
- `UROUTEBLOCK src_block, dst_block`
- `UMERGEBLOCK block_id`

这一层的意义是：

> **把 block 和 graph 正式提升为一等执行对象。**

也就是说，URX 不再只是“指令流”，还开始直接表达：

- block 结构
- 图边关系
- merge 模式
- graph 绑定
- graph 执行

---

## URX v0.3 之后的内部优化语义

再往后，URX 体系里已经进入了一批更偏引擎内部的操作意图：

- `UFUSE`
- `UPARTITION`
- `UREUSEPATH`
- `UBINDPART`
- `UMERGEPART`

这些操作目前更适合作为：

> **引擎内部语义 / runtime 内部操作**

而不是一开始就暴露给最终用户的文本指令。

但它们已经明确属于 URX 体系，因为它们负责：

- block fusion
- graph partition
- inertia-aware reuse
- partition-level binding
- partition-level merge

换句话说，URX 不只是一套“执行指令”，还逐渐成为一套：

> **图优化 + 图执行 + 图合并** 的统一语义层。

---

## URX 与 IR Graph 的关系

URX 的真正入口不是单条 opcode，而是 **IR Graph**。

定义：

\[
\mathcal{G}_{IR}=(V_{IR},E_{IR})
\]

其中：

- \(V_{IR}\)：IR Block 集合
- \(E_{IR}\)：块之间的数据 / 控制依赖边

在 URX 的体系里，任务通常经历：

\[
Task \to IRBlock \to IRGraph \to Partition \to Binding \to Execution
\]

因此，URX 的最关键特征之一是：

> **先图，后执行。**

这点和很多传统作业调度器完全不同。  
传统系统先有 job，再分配资源；  
URX 则先把任务结构化，再决定怎样进入执行宇宙。

---

## URX 与 Runtime 的关系

URX 自己不是全部，它必须通过 runtime 落地。  
而 runtime 的方向已经被前面的版本逐渐明确：

- packet-first
- local-ring-first
- zero-copy 风格
- reducer-based merge
- policy-based binding
- topology-aware cost
- reservation / backfill 留口

所以可以写成：

\[
URX + Runtime = Executable\ Node\ Universe
\]

换句话说：

- URX 给出执行语言；
- Runtime 把语言变成真的执行路径。

---

## URX 的关键技术方向

### 1. Packet-first
执行对象尽量先进入 packet / buffer，而不是在各层之间大量搬动大对象。

### 2. Local-ring-first
同宿主节点优先走 ring，避免不必要的 socket 化开销。

### 3. Reducer-based merge
合并不再写死，而通过 reducer trait 做统一扩展。

### 4. Policy-based scheduling
节点与 partition 的绑定不写死在 runtime 内部，而通过 policy trait 挂接。

### 5. Inertia-aware reuse
执行系统显式考虑历史轨道，而不是只看“当前谁空闲”。

---

## URX 与其他体系的区别

### 与 QEMU 的区别
QEMU 更像虚拟 CPU / 模拟器工法。  
URX 则是上位执行语义层。

### 与 Slurm 的区别
Slurm 的核心是作业调度。  
URX 的核心是节点宇宙的执行语义与路径组织。

### 与 EasyTier 的区别
EasyTier 提供的是数据通路启发，例如：

- ring tunnel
- zero-copy
- packet-first

URX 借的是其数据路径思想，而不是其 VPN 产品定位。

### 与普通云超算的区别
普通云超算强调资源规模。  
URX 更强调：

- 执行语义
- 图结构
- 节点宇宙
- 路由与合并
- 可计算宇宙

---

## URX 的真正目标

如果把 URX 只说成“更强的调度框架”，那仍然太低。  
它更准确的目标是：

> **为可计算宇宙建立统一的中层执行语言。**

也就是说：

- 不是把更多机器接起来就结束；
- 不是把更多算力堆起来就结束；
- 而是重新定义什么可以被表达、绑定、传递、执行与合并。

这也是 URX 最大的野心所在。

---

## 一句话总结

**URX 不是普通指令集，而是面向节点宇宙、图执行、packet 路径与调度策略的统一扩展执行语义。**

再压短一点就是：

> **URX 的作用，不是给某颗 CPU 发命令，而是给整个可计算宇宙建立执行语言。**
