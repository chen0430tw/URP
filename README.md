# URX Runtime v1.1

通用拟构扩展指令体系（URX）的 Rust Runtime 实现。

## 快速开始

### 构建项目

```bash
# 仅 CPU（默认）
cargo build

# 启用 GPU 后端（需要支持 Vulkan/Metal/DX12 的显卡）
cargo build --features gpu

# 启用 USB 硬件后端（需要 serialport 库）
cargo build --features usb

# 全部启用
cargo build --features gpu,usb
```

### 运行测试

```bash
# 所有 CPU 测试
cargo test

# 含 GPU 测试
cargo test --features gpu

# 含 USB 协议测试
cargo test --features usb

# 带输出
cargo test -- --nocapture
```

### 运行演示

```bash
cargo run
```

输出 Demo A–L：覆盖 CPU 执行、浮点流水线、ET-WCN 冷却、调度策略、
ZeroCopyContext、JIT 编译、USB 协议、资源预留，以及从文件加载真实工作负载图。

---

## 项目概述

URX Runtime 是一个面向"可计算宇宙"的执行底座，核心理念是：

> **万物皆为可调度的算力节点**

计算任务被表示为 IRGraph，经由融合、分区、调度后分发到 CPU、GPU 或 USB 连接的外部设备执行，通过数据包传递中间结果，最终合并输出。

```
IRGraph → fusion → partition → policy-based binding
        → execute (CPU / GPU / USB) → packet route → reducer merge
```

---

## 核心特性

| 特性 | 说明 |
|------|------|
| IRGraph 执行模型 | 任务先被结构化为有向图，再执行 |
| Block Fusion | 自动融合相邻可合并块 |
| Graph Partition | 按资源需求分区 |
| Policy-based Scheduling | 可替换调度策略（Multifactor / Round-Robin 等） |
| PartitionDAGScheduler | 完整 DAG 拓扑调度，AsyncLane 并行执行分区 |
| ET-WCN Cooling | 基于熵温度冷却的分区绑定优化器 |
| Topology-aware Cost Model | 考虑区域、带宽、惯性的成本模型 |
| IRGraph JSON 序列化 | serde_json 双向：`from_json` / `to_json` / `load_json` / `save_json` |
| ZeroCopyContext | SharedMemoryRegion + BufferPool + InertiaBufferCache 统一门面 |
| GPU Executor | wgpu WGSL 着色器执行（feature="gpu"） |
| JIT Compiler | IRGraph → WGSL 着色器动态编译（feature="gpu"） |
| USB Executor | 通过 CDC 串口协议驱动外部计算设备（feature="usb"） |
| Pico 固件 | RP2040 参考固件，实现完整 URP 二进制线路协议 |
| KDMapper FFI | 内核驱动加载通道（feature="kdmapper"） |

---

## 主要模块

| 模块 | 文件 | 功能 |
|------|------|------|
| Runtime | `src/runtime.rs` | 端到端执行引擎，含 ET-WCN 集成、PartitionDAGScheduler 集成 |
| IR | `src/ir.rs` | IRGraph、IRBlock、Opcode（40+ 条指令）+ JSON 序列化 |
| Node | `src/node.rs` | 节点模型：Cpu / Gpu / Qcu / Memory / Network / Rule / Structure / Usb |
| Executor | `src/executor.rs` | CPU 块执行器，支持所有整型/浮点 opcode |
| GPU Executor | `src/gpu_executor.rs` | wgpu 计算着色器执行器（feature="gpu"） |
| JIT Compiler | `src/jit_compiler.rs` | IRGraph → WGSL 动态编译（feature="gpu"） |
| USB Executor | `src/usb_executor.rs` | USB CDC 二进制协议、设备发现、UsbExecutor |
| ET Cooling | `src/et_cooling.rs` | 熵温度 WCN 冷却分区优化器 |
| Scheduler | `src/scheduler.rs` | PartitionDAGScheduler + AsyncLane DAG 拓扑调度 |
| Policy | `src/policy.rs` | 调度策略接口和内置实现 |
| Cost | `src/cost.rs` | 拓扑感知成本模型 |
| Optimizer | `src/optimizer.rs` | 图优化（融合、分区） |
| Partition | `src/partition.rs` | 分区绑定逻辑 |
| Reservation | `src/reservation.rs` | 资源预留和回填 |
| Packet | `src/packet.rs` | 数据包结构和编解码 |
| Ring | `src/ring.rs` | 本地环形通道 |
| Remote | `src/remote.rs` | 远程网络通信 |
| Reducer | `src/reducer.rs` | 结果合并语义 |
| Shared Memory | `src/shared_memory.rs` | 跨节点共享内存抽象（SharedMemoryRegion / BufferPool / ZeroCopyContext） |
| KDMapper FFI | `src/kdmapper_ffi.rs` | 内核驱动加载 FFI（feature="kdmapper"） |

---

## 指令集（Opcode）

> ⚠️ 命名规则：`U*` = 整型（i64），`F*` = 浮点（f64）。不存在 `UConstF64` 或 `UAddF64`，请勿在计算图 JSON 中使用。

### 整型常量
`UConstI64(i64)` `UConstStr(String)`

### 整型算术
`UAdd` `USub` `UMul` `UDiv` `URem`

### 整型比较（→ i64: 1/0）
`UCmpEq` `UCmpLt` `UCmpLe`

### 整型逻辑 / 位运算
`UAnd` `UOr` `UXor` `UNot` `UShl` `UShr` `UShra`

### 字符串操作
`UConcat` `UI64ToStr` `UStrToI64` `UStrLen` `UStrSlice` `UStrSplit`

### 条件 / 聚合
`USelect` `UMin` `UMax` `UAbs` `UAssert`

### 浮点常量
`FConst(f64)`

### 浮点二元运算
`FAdd` `FSub` `FMul` `FDiv` `FPow`

### 浮点一元运算
`FSqrt` `FAbs` `FNeg` `FFloor` `FCeil` `FRound`

### 浮点比较（→ i64: 1/0）
`FCmpEq` `FCmpLt` `FCmpLe`

### 类型转换
`F64ToI64` `I64ToF64`

---

## Feature Flags

| Feature | 说明 | 额外依赖 |
|---------|------|---------|
| `gpu` | 启用 WgpuExecutor 和 JitExecutor | wgpu, bytemuck, pollster |
| `usb` | 启用 UsbExecutor 和 UsbDiscovery | serialport |
| `kdmapper` | 启用 KDMapper FFI 类型（纯 Rust） | — |
| `kdmapper-native` | 启用 C++ 原生链接（需要预构建 DLL） | libloading |

---

## 使用示例

### 基础 CPU 执行

```rust
use urx_runtime_v08::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut graph = IRGraph::new();

    let b1 = IRBlock::new("x", Opcode::UConstI64(10));
    let b2 = IRBlock::new("y", Opcode::UConstI64(32));
    let b3 = IRBlock::new("sum", Opcode::UAdd);
    graph.blocks.push(b1);
    graph.blocks.push(b2);
    graph.blocks.push(b3);
    graph.edges.push(IREdge {
        src_block: "x".into(), dst_block: "sum".into(),
        output_key: "out".into(), input_key: "a".into(),
    });
    graph.edges.push(IREdge {
        src_block: "y".into(), dst_block: "sum".into(),
        output_key: "out".into(), input_key: "b".into(),
    });

    let node = Node::new("cpu0", NodeType::Cpu, 100.0);
    let mut rt = URXRuntime::new(vec![node], MultifactorPolicy::new());

    let result = rt.execute_graph(&graph).await;
    println!("{:?}", result); // RuntimeResult { ... }
    Ok(())
}
```

### IRGraph JSON 序列化

```rust
// 序列化到 JSON 字符串
let json = graph.to_json()?;

// 从 JSON 字符串反序列化
let graph2 = IRGraph::from_json(&json)?;

// 从文件加载
let graph3 = IRGraph::load_json("C:/Users/asus/urp/fft_n64_s6.json")?;

// 保存到文件
graph.save_json("output.json")?;
```

### 浮点 L2 范数

```rust
// 计算 sqrt(3.0^2 + 4.0^2) = 5.0
let mut g = IRGraph::new();
g.blocks.push(IRBlock::new("x",  Opcode::FConst(3.0)));
g.blocks.push(IRBlock::new("y",  Opcode::FConst(4.0)));
g.blocks.push(IRBlock::new("x2", Opcode::FMul));
g.blocks.push(IRBlock::new("y2", Opcode::FMul));
g.blocks.push(IRBlock::new("s",  Opcode::FAdd));
g.blocks.push(IRBlock::new("r",  Opcode::FSqrt));
// ... 添加边 ...
```

### ET-WCN 冷却优化

```rust
let mut rt = URXRuntime::new(nodes, MultifactorPolicy::new());
rt.set_et_policy(ETCoolingPolicy::new(ETWCNCooling::default()));
// 后续 execute_graph() 自动使用熵温度感知的分区绑定
```

### GPU 执行（feature="gpu"）

```rust
#[cfg(feature = "gpu")]
{
    let executor = WgpuExecutor::new().await?;
    let inputs: Vec<f32> = (0..1024).map(|i| i as f32).collect();
    let result = executor.run_add(&inputs, &inputs).await?;
}
```

### JIT 编译（feature="gpu"）

```rust
#[cfg(feature = "gpu")]
{
    let compiled = compile_graph(&graph)?;
    println!("n_regs={}, wgsl_preview={}", compiled.n_regs, &compiled.wgsl[..100]);
}
```

### USB 设备发现与执行（feature="usb"）

> **注意**：需要 CDC ACM 串口类设备（如烧录了 urp-pico 固件的 Raspberry Pi Pico）。
> 普通 U 盘（MSC 存储类，挂载为盘符）不会出现在串口列表中，无法被识别。

```rust
#[cfg(feature = "usb")]
{
    use urx_runtime_v08::{UsbDiscovery, UsbExecutor, UsbDeviceConfig};

    let devices = UsbDiscovery::scan(std::time::Duration::from_secs(2))?;
    for d in &devices {
        println!("{}: throughput={} caps={:#x}", d.name, d.throughput, d.caps);
    }

    let cfg = UsbDeviceConfig { port: "/dev/ttyACM0".into(), baud: 115200 };
    let mut exec = UsbExecutor::open(cfg)?;
    let response = exec.transact(UsbOpcodeId::UAdd, &[
        &PayloadValue::I64(10),
        &PayloadValue::I64(32),
    ])?;
    println!("{:?}", response); // I64(42)
}
```

### 自定义调度策略

```rust
struct MyPolicy;

impl SchedulerPolicy for MyPolicy {
    fn select_node<'a>(&self, block: &IRBlock, nodes: &'a [Node]) -> Option<&'a Node> {
        nodes.iter().max_by(|a, b|
            a.compute_capacity.partial_cmp(&b.compute_capacity).unwrap()
        )
    }
}

let mut rt = URXRuntime::new(nodes, MyPolicy);
```

---

## 演示列表（Demo A–L）

运行 `cargo run` 可执行全部演示：

| Demo | 内容 |
|------|------|
| A | 基础 CPU 执行：UConstI64 + UAdd → 42 |
| B | 浮点 L2 范数：FConst + FMul + FAdd + FSqrt → 5.0 |
| C | ET-WCN 冷却：熵温度感知分区绑定 |
| D | 调度策略：MultifactorPolicy 标签匹配、区域偏好、惯性亲和 |
| E | 资源预留：ReservationTable + BackfillWindow |
| F | 远程数据包路由：RemotePacketLink |
| G | 成本模型：node_score + route_cost |
| H | ZeroCopyContext：SharedMemoryRegion + BufferPool + InertiaBufferCache |
| I | JIT 编译（CPU 路径）：IRGraph → CompiledGraph + WGSL 预览 |
| J | USB 协议层：crc8 / encode_request / decode_response + UsbLoopbackExecutor |
| K | ReservationAwarePolicy：自定义预留感知策略，最早空闲节点选择 |
| L | 真实工作负载图：加载并执行 FFT-64/128、Transformer 注意力、ResNet-8 |

---

## 工作站 Agent 操作指南

本节说明如何为 URX Runtime 生成可直接加载的工作负载计算图 JSON，供 Demo L 执行。

### 文件路径

工作负载图保存到：`C:\Users\asus\urp\`

Demo L 期望以下文件：
- `fft_n64_s6.json` — FFT，N=64，6 个蝶形级
- `fft_n128_s7.json` — FFT，N=128，7 个蝶形级
- `attn_h4_s32.json` — Transformer 自注意力，4 头，序列长 32
- `resnet_8blk_c64.json` — ResNet 残差块，8 块，64 通道

### JSON 格式

```json
{
  "graph_id": "fft_n64_s6",
  "blocks": [
    { "block_id": "c0", "opcode": { "FConst": 1.0 },
      "inputs": [], "output": "c0", "required_tag": "",
      "merge_mode": "List", "resource_shape": "", "preferred_zone": "",
      "inertia_key": null, "estimated_duration": 1 },
    { "block_id": "add0", "opcode": "FAdd",
      "inputs": ["c0", "c1"], "output": "add0", "required_tag": "",
      "merge_mode": "List", "resource_shape": "", "preferred_zone": "",
      "inertia_key": null, "estimated_duration": 1 }
  ],
  "edges": [
    { "src_block": "c0", "dst_block": "add0",
      "output_key": "out", "input_key": "a" }
  ]
}
```

### ⚠️ Opcode 命名规则（严格遵守）

| 正确 | 错误（不存在） |
|------|--------------|
| `"FConst": 3.14` | `"UConstF64": 3.14` |
| `"UConstI64": 42` | `"UConst": 42` |
| `"FAdd"` | `"UAddF64"` |
| `"FMul"` | `"UMulF64"` |
| `"FSqrt"` | `"USqrtF64"` |
| `"FCmpLt"` | `"UCmpLtF64"` |
| `"I64ToF64"` | `"UI64ToF64"` |
| `"F64ToI64"` | `"UF64ToI64"` |

ReLU 正确写法（无原生 ReLU opcode）：
```json
{ "block_id": "relu", "opcode": "USelect",
  "inputs": ["cmp_gt_zero", "x", "zero_const"] }
```
其中 `cmp_gt_zero` = `FCmpLt(FConst(0.0), x)`，条件为 cond≠0 → input[1]，else → input[2]。

### 运行步骤

```bash
# 1. 将生成的 JSON 文件放到工作站目录
# （已由 Agent 完成）

# 2. 运行 Demo L
cargo run 2>&1 | grep -A 20 "Demo L"
```

---

## 代码结构

```
URP/
├── src/
│   ├── lib.rs               # 库入口，公共 API 导出
│   ├── main.rs              # Demo A–L 示例入口
│   ├── runtime.rs           # 核心运行时 + ET-WCN + PartitionDAGScheduler 集成
│   ├── ir.rs                # IRGraph / IRBlock / Opcode + JSON 序列化
│   ├── node.rs              # 节点模型（含 NodeType::Usb）
│   ├── executor.rs          # CPU 执行器（所有 opcode）
│   ├── gpu_executor.rs      # GPU 执行器（feature="gpu"）
│   ├── jit_compiler.rs      # IRGraph→WGSL JIT（feature="gpu"）
│   ├── usb_executor.rs      # USB CDC 协议 + 设备发现（feature="usb"）
│   ├── et_cooling.rs        # ET-WCN 冷却优化器
│   ├── scheduler.rs         # PartitionDAGScheduler + AsyncLane
│   ├── policy.rs            # 调度策略
│   ├── cost.rs              # 成本模型
│   ├── optimizer.rs         # 图融合/分区
│   ├── partition.rs         # 分区绑定
│   ├── reservation.rs       # 资源预留
│   ├── packet.rs            # 数据包
│   ├── ring.rs              # 本地环形通道
│   ├── remote.rs            # 远程通信
│   ├── reducer.rs           # 结果合并
│   ├── shared_memory.rs     # 共享内存抽象
│   └── kdmapper_ffi.rs      # KDMapper FFI（feature="kdmapper"）
├── tests/
│   ├── integration_test.rs  # 端到端集成测试
│   ├── opcode_test.rs       # 指令集单元测试
│   ├── opcode_batch34_test.rs
│   ├── float_test.rs        # 浮点 opcode 测试
│   ├── executor_test.rs     # Executor 测试
│   ├── scheduling_test.rs   # 调度策略测试
│   ├── json_schema_test.rs  # IRGraph JSON 序列化测试
│   ├── gpu_test.rs          # GPU 执行测试（feature="gpu"）
│   ├── gpu_schedule_test.rs # GPU 调度测试（feature="gpu"）
│   ├── jit_integration_test.rs # JIT 端到端测试（feature="gpu"）
│   ├── usb_protocol_test.rs # USB 协议交叉验证（30 个测试）
│   ├── bench_test.rs        # 性能基准
│   └── kdmapper_test.rs     # KDMapper FFI 测试
├── firmware/
│   └── urp-pico/            # RP2040 参考固件（TinyUSB CDC）
├── docs/
│   ├── CHANGELOG_URX.md
│   ├── URX_Claude_开发者接手说明.md
│   └── 系统能力概述.md
├── Cargo.toml
└── README.md
```

---

## USB 线路协议

所有 URP 设备使用统一的二进制帧格式：

```
[SYNC=0xA5] [LEN_LO] [LEN_HI] [PAYLOAD...] [CRC8]
```

- **CRC8**: poly=0x31，MSB-first，init=0x00
- **请求 PAYLOAD**: `OPCODE(1B) | N_IN(1B) | [IN_LEN(2B LE) | IN_BYTES]...`
- **响应 PAYLOAD**: `STATUS(1B) | [OUT_LEN(2B LE) | OUT_BYTES]`
- **STATUS**: 0x00=OK，0x01=Unsupported，0x02=ArgError

### 值编码（PayloadValue）

| 类型 | 标签字节 | 编码 |
|------|---------|------|
| I64 | 0x01 | 8B little-endian |
| F64 | 0x04 | 8B little-endian IEEE 754 |
| Str | 0x02 | 4B LE 长度 + UTF-8 字节 |

---

## Pico 固件

`firmware/urp-pico/` 为 Raspberry Pi Pico（RP2040）提供完整参考实现：

- **VID/PID**: 0x2E8A / 0x000A（由 UsbDiscovery 自动识别）
- **串口**: USB CDC ACM，无需额外驱动（Windows/Linux/macOS）
- **支持全部 40+ 条 URX opcode**

构建方式：
```bash
cd firmware/urp-pico
mkdir build && cd build
cmake -DPICO_SDK_PATH=/path/to/pico-sdk ..
make -j4
# 生成 urp_pico.uf2，拖拽到 Pico 即可
```

---

## 版本历史

### v1.1（当前）
- **PartitionDAGScheduler 完整集成** — DAG 拓扑调度替代 sequential 执行，AsyncLane 并行分区
- **IRGraph JSON 序列化** — serde 派生，`from_json` / `to_json` / `load_json` / `save_json`
- **ZeroCopyContext 演示** — SharedMemoryRegion / BufferPool / InertiaBufferCache 统一门面（Demo H）
- **JIT 编译 CPU 路径演示** — `compile_graph()` CompiledGraph 元数据验证（Demo I）
- **USB 协议层演示** — crc8 / encode_request / decode_response / UsbLoopbackExecutor（Demo J）
- **ReservationAwarePolicy 演示** — 自定义最早空闲策略（Demo K）
- **真实工作负载图加载** — FFT-64/128、Transformer 注意力、ResNet-8，从 JSON 文件加载执行（Demo L）
- **死代码清理** — 删除 AsyncLane.id 字段，统一 `#![allow(dead_code)]`
- **json_schema_test.rs** — IRGraph JSON 往返测试

### v1.0
- USB 外部计算节点（NodeType::Usb、UsbExecutor、UsbDiscovery）
- USB 线路协议（SYNC/LEN/PAYLOAD/CRC8 帧，完整 opcode 编码）
- RP2040 参考固件（firmware/urp-pico/，TinyUSB CDC ACM）
- 协议交叉验证（usb_protocol_test.rs，30 个测试）
- JIT 编译器（IRGraph → WGSL 动态编译，确定性寄存器分配）
- GPU 执行器（WgpuExecutor，WGSL 着色器，feature="gpu"）
- ET-WCN 冷却（熵温度感知分区绑定优化器）
- 浮点指令集（FConst/FAdd/FSub/FMul/FDiv/FPow/FSqrt/FAbs/FNeg/FFloor/FCeil/FRound + 比较 + 类型转换）
- 完整测试覆盖（12 个测试文件，涵盖所有后端和 opcode）

### v0.9
- KDMapper FFI 集成（内核驱动加载通道）
- 34 个测试全部通过

### v0.8
- Scheduler policy trait
- Topology-aware cost model
- Reservation / backfill skeleton

### v0.7
- Remote packet path
- Partition-level binding
- Reducer trait

### v0.6
- Rust 版 fusion + partition + inertia-aware reuse

### v0.5
- Rust 版 IRBlock → packet → ring → merge 端到端骨架

---

## 许可证

MIT License
