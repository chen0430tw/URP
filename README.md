# URX Runtime Rust v0.8

这版继续把 runtime 往“真正可调度底座”推进，新增：

- **scheduler policy trait**
- **topology-aware cost model**
- **reservation / backfill skeleton**

现在链路已经变成：

**IRGraph -> fusion -> partition -> policy-based binding -> execute -> local/remote packet route -> reducer merge**

## 新增重点

### 1. SchedulerPolicy trait
把“怎么选节点/分区绑定”从硬编码推进成可替换策略。

### 2. Topology-aware cost model
把 host / zone / bandwidth / inertia 纳入统一评分。

### 3. Reservation / backfill skeleton
引入面向 future work 的保留与回填接口，为以后做 Slurm-like 深化留口。

## 主要模块

- `src/policy.rs`
  - `SchedulerPolicy`
  - `MultifactorPolicy`
- `src/cost.rs`
  - topology-aware score / route cost
- `src/reservation.rs`
  - reservation table
  - backfill skeleton
- `src/runtime.rs`
  - 端到端 runtime 接入 policy
