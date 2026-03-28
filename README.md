# URX Runtime v1.0

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

## 项目概述

URX Runtime 是一个面向"可计算宇宙"的执行底座，核心理念是：

> **万物皆为可调度的算力节点**

计算任务被表示为 IRGraph，经由融合、分区、调度后分发到 CPU、GPU 或 USB 连接的外部设备执行，通过数据包传递中间结果，最终合并输出。

```
IRGraph → fusion → partition → policy-based binding
        → execute (CPU / GPU / USB) → packet route → reducer merge
```

## 核心特性

| 特性 | 说明 |
|------|------|
| IRGraph 执行模型 | 任务先被结构化为有向图，再执行 |
| Block Fusion | 自动融合相邻可合并块 |
| Graph Partition | 按资源需求分区 |
| Policy-based Scheduling | 可替换调度策略（Multifactor / Round-Robin 等） |
| ET-WCN Cooling | 基于熵温度冷却的分区绑定优化器 |
| Topology-aware Cost Model | 考虑区域、带宽、惯性的成本模型 |
| GPU Executor | wgpu WGSL 着色器执行（feature="gpu"） |
| JIT Compiler | IRGraph → WGSL 着色器动态编译（feature="gpu"） |
| USB Executor | 通过 CDC 串口协议驱动外部计算设备（feature="usb"） |
| Pico 固件 | RP2040 参考固件，实现完整 URP 二进制线路协议 |
| KDMapper FFI | 内核驱动加载通道（feature="kdmapper"） |

## 主要模块

| 模块 | 文件 | 功能 |
|------|------|------|
| Runtime | `src/runtime.rs` | 端到端执行引擎，含 ET-WCN 集成 |
| IR | `src/ir.rs` | 中间表示：IRGraph、IRBlock、Opcode（40+ 条指令） |
| Node | `src/node.rs` | 节点模型：Cpu / Gpu / Qcu / Memory / Network / Rule / Structure / Usb |
| Executor | `src/executor.rs` | CPU 块执行器，支持所有整型/浮点 opcode |
| GPU Executor | `src/gpu_executor.rs` | wgpu 计算着色器执行器（feature="gpu"） |
| JIT Compiler | `src/jit_compiler.rs` | IRGraph → WGSL 动态编译（feature="gpu"） |
| USB Executor | `src/usb_executor.rs` | USB CDC 二进制协议、设备发现、UsbExecutor |
| ET Cooling | `src/et_cooling.rs` | 熵温度 WCN 冷却分区优化器 |
| Scheduler | `src/scheduler.rs` | 调度器核心逻辑 |
| Policy | `src/policy.rs` | 调度策略接口和内置实现 |
| Cost | `src/cost.rs` | 拓扑感知成本模型 |
| Optimizer | `src/optimizer.rs` | 图优化（融合、分区） |
| Partition | `src/partition.rs` | 分区绑定逻辑 |
| Reservation | `src/reservation.rs` | 资源预留和回填 |
| Packet | `src/packet.rs` | 数据包结构和编解码 |
| Ring | `src/ring.rs` | 本地环形通道 |
| Remote | `src/remote.rs` | 远程网络通信 |
| Reducer | `src/reducer.rs` | 结果合并语义 |
| Shared Memory | `src/shared_memory.rs` | 跨节点共享内存抽象 |
| KDMapper FFI | `src/kdmapper_ffi.rs` | 内核驱动加载 FFI（feature="kdmapper"） |

## 指令集（Opcode）

### 整型运算
`UConstI64` `UAdd` `USub` `UMul` `UDiv` `UMod` `UNeg` `UAbs`

### 浮点运算
`UConstF64` `UAddF64` `USubF64` `UMulF64` `UDivF64` `USqrtF64` `UAbsF64` `UNegF64` `UFloorF64` `UCeilF64` `URoundF64` `UExpF64` `ULnF64` `UPowF64` `USinF64` `UCosinF64`

### 比较与逻辑
`UEq` `UNeq` `ULt` `UGt` `ULe` `UGe` `UAnd` `UOr` `UNot` `UXor`

### 位运算
`UShl` `UShr` `UBitAnd` `UBitOr` `UBitXor` `UBitNot`

### 范围 / 聚合
`UMin` `UMax` `UClamp` `USum` `UMean`

### 类型转换
`UI64ToF64` `UF64ToI64`

### 控制流 / 特殊
`USelect` `UAssert` `UNoop` `UHalt`

## Feature Flags

| Feature | 说明 | 额外依赖 |
|---------|------|---------|
| `gpu` | 启用 WgpuExecutor 和 JitExecutor | wgpu, bytemuck, pollster |
| `usb` | 启用 UsbExecutor 和 UsbDiscovery | serialport |
| `kdmapper` | 启用 KDMapper FFI 类型（纯 Rust） | — |
| `kdmapper-native` | 启用 C++ 原生链接（需要预构建 DLL） | libloading |

## 使用示例

### 基础 CPU 执行

```rust
use urx_runtime::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut graph = IRGraph::new();

    let b1 = IRBlock::new("x", Opcode::UConstI64(10));
    let b2 = IRBlock::new("y", Opcode::UConstI64(32));
    let b3 = IRBlock::new("sum", Opcode::UAdd);
    graph.add_block(b1);
    graph.add_block(b2);
    graph.add_block(b3);
    graph.add_edge("x", "sum");
    graph.add_edge("y", "sum");

    let cpu = Node::new("cpu0", NodeType::Cpu, 100);
    let mut rt = URXRuntime::new(MultifactorPolicy::default());
    rt.add_node(cpu);

    let result = rt.execute_graph(&graph).await?;
    println!("{:?}", result); // I64(42)
    Ok(())
}
```

### 浮点 L2 范数

```rust
let mut graph = IRGraph::new();
// ... 构建 sqrt(x*x + y*y) 的图
let result = rt.execute_graph(&graph).await?;
// F64(5.0)
```

### ET-WCN 冷却优化

```rust
let mut rt = URXRuntime::new(MultifactorPolicy::default());
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
    use urx_runtime::compile_graph;
    let compiled = compile_graph(&graph)?;
    let executor = JitExecutor::new().await?;
    let result = executor.run(&compiled, &inputs).await?;
}
```

### USB 设备发现与执行（feature="usb"）

> **注意**：需要 CDC ACM 串口类设备（如烧录了 urp-pico 固件的 Raspberry Pi Pico）。
> 普通 U 盘（MSC 存储类，挂载为盘符）不会出现在串口列表中，无法被识别。

```rust
#[cfg(feature = "usb")]
{
    use urx_runtime::{UsbDiscovery, UsbExecutor};

    // 自动扫描并探测所有已知 VID/PID 的 URP 设备
    let devices = UsbDiscovery::scan(std::time::Duration::from_secs(2))?;
    for d in &devices {
        println!("{}: throughput={} caps={:#x}", d.name, d.throughput, d.caps);
    }

    // 直接打开指定串口
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
    fn select_node(&self, block: &IRBlock, nodes: &[Node], costs: &CostMap) -> Option<Node> {
        nodes.iter().max_by_key(|n| n.capacity).cloned()
    }
}

let mut rt = URXRuntime::new(MyPolicy);
```

## 代码结构

```
URP/
├── src/
│   ├── lib.rs               # 库入口，公共 API 导出
│   ├── main.rs              # 示例入口
│   ├── runtime.rs           # 核心运行时 + ET-WCN 集成
│   ├── ir.rs                # IRGraph / IRBlock / Opcode
│   ├── node.rs              # 节点模型（含 NodeType::Usb）
│   ├── executor.rs          # CPU 执行器（所有 opcode）
│   ├── gpu_executor.rs      # GPU 执行器（feature="gpu"）
│   ├── jit_compiler.rs      # IRGraph→WGSL JIT（feature="gpu"）
│   ├── usb_executor.rs      # USB CDC 协议 + 设备发现（feature="usb"）
│   ├── et_cooling.rs        # ET-WCN 冷却优化器
│   ├── scheduler.rs         # 调度器
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
│   ├── opcode_batch34_test.rs # 第34批 opcode 测试
│   ├── float_test.rs        # 浮点 opcode 测试
│   ├── executor_test.rs     # Executor 测试
│   ├── scheduling_test.rs   # 调度策略测试
│   ├── gpu_test.rs          # GPU 执行测试（feature="gpu"）
│   ├── gpu_schedule_test.rs # GPU 调度测试（feature="gpu"）
│   ├── jit_integration_test.rs # JIT 端到端测试（feature="gpu"）
│   ├── usb_protocol_test.rs # USB 协议交叉验证（30 个测试）
│   ├── bench_test.rs        # 性能基准
│   └── kdmapper_test.rs     # KDMapper FFI 测试
├── firmware/
│   └── urp-pico/            # RP2040 参考固件（TinyUSB CDC）
│       ├── CMakeLists.txt
│       └── src/
│           ├── main.c           # 主循环 + 全 opcode 实现
│           ├── tusb_config.h    # TinyUSB 配置
│           ├── usb_descriptors.h/.c  # USB 描述符（VID=0x2E8A PID=0x000A）
│           └── CMakeLists.txt
├── Cargo.toml
└── README.md
```

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

### HELLO 握手（Opcode=0xF0）

设备返回 Str 载荷：
```
name=urp-pico-XXXX\ncaps=0x10-0x51\nthroughput=133000\n
```

## Pico 固件

`firmware/urp-pico/` 为 Raspberry Pi Pico（RP2040）提供完整参考实现：

- **VID/PID**: 0x2E8A / 0x000A（由 UsbDiscovery 自动识别）
- **串口**: USB CDC ACM，无需额外驱动（Windows/Linux/macOS）
- **序列号**: 从 Flash UID 自动生成
- **支持全部 40+ 条 URX opcode**
- **与 usb_protocol_test.rs 的 firmware_process() 逻辑完全对齐**

构建方式：
```bash
cd firmware/urp-pico
mkdir build && cd build
cmake -DPICO_SDK_PATH=/path/to/pico-sdk ..
make -j4
# 生成 urp_pico.uf2，拖拽到 Pico 即可
```

## 版本历史

### v1.0（当前）
- **USB 外部计算节点** — NodeType::Usb、UsbExecutor、UsbDiscovery
- **USB 线路协议** — SYNC/LEN/PAYLOAD/CRC8 帧，完整 opcode 编码
- **RP2040 参考固件** — firmware/urp-pico/，TinyUSB CDC ACM
- **协议交叉验证** — usb_protocol_test.rs，30 个测试，固件逻辑 Rust 镜像
- **JIT 编译器** — IRGraph → WGSL 动态编译，确定性寄存器分配
- **GPU 执行器** — WgpuExecutor，WGSL 着色器，feature="gpu"
- **ET-WCN 冷却** — 熵温度感知分区绑定优化器
- **浮点指令集** — 16 条 F64 opcode，包括三角函数、指数、幂次
- **完整测试覆盖** — 12 个测试文件，涵盖所有后端和 opcode

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

## 许可证

MIT License
