# URX Changelog

## v1.0（当前）

### USB 外部计算节点
- `NodeType::Usb` — 新节点类型，可接入 USB CDC 连接的外部计算设备
- `UsbExecutor`（feature="usb"）— serialport 驱动的真实硬件执行器
- `UsbLoopbackExecutor` — 无需物理设备的回环测试执行器
- `UsbDiscovery`（feature="usb"）— 自动扫描所有串口，通过 HELLO 握手识别 URP 设备
- `DeviceInfo` — 解析设备名称、能力位图、吞吐量，`compute_capacity()` 映射为调度权重
- 已知 VID/PID 过滤：RP2040=0x2E8A，STM32=0x0483，Pi=0x1D6B

### USB 线路协议
- 帧格式：SYNC(0xA5) | LEN(2B LE) | PAYLOAD | CRC8(poly=0x31, MSB-first, init=0x00)
- 请求：OPCODE(1B) | N_IN(1B) | [IN_LEN(2B LE) | IN_BYTES]...
- 响应：STATUS(1B) | [OUT_LEN(2B LE) | OUT_BYTES]
- PayloadValue 编码：TYPE_I64=0x01+8B LE，TYPE_F64=0x04+8B LE，TYPE_STR=0x02+4B len+UTF-8
- HELLO 握手（Opcode=0xF0）：返回设备名、能力位图、吞吐量

### RP2040 参考固件（firmware/urp-pico/）
- TinyUSB CDC ACM，VID=0x2E8A / PID=0x000A
- 序列号从 Flash UID 自动生成（`pico_get_unique_board_id_string`）
- RX 状态机：ST_SYNC → ST_LEN0 → ST_LEN1 → ST_PAYLOAD → ST_CRC
- 实现全部 40+ 条 URX opcode，含 USelect（3 输入）
- `cdc_write_all()` — 分块写入 + 强制 `tud_cdc_write_flush()`
- 与 `tests/usb_protocol_test.rs` 的 firmware_process() 逻辑完全对齐

### 协议交叉验证
- `tests/usb_protocol_test.rs`：30 个测试
  - firmware_process()：Rust 语言镜像 main.c 逻辑，无需物理硬件
  - 覆盖：全部整型算术/逻辑/位移/比较、全部浮点 opcode、类型转换、HELLO、错误路径、L2 范数管线

### JIT 编译器（feature="gpu"）
- `compile_graph()` — IRGraph → WGSL 着色器 + 元数据
- 拓扑排序：Kahn 算法，每波按字母序排序，确保确定性寄存器分配
- 支持 opcode：UAdd/USub/UMul/UDiv/UMod/UAbs/UNeg/USqrtF64/USelect/UAssert 等
- `JitExecutor`：编译后直接在 GPU 上运行
- `tests/jit_integration_test.rs`：CPU 路径 + GPU 路径测试

### GPU 执行器（feature="gpu"）
- `WgpuExecutor`：基于 wgpu 的 WGSL 计算着色器
- `run_add()` / `run_sqrt()` 批量向量运算，N 可达 1M+
- `tests/gpu_test.rs`、`tests/gpu_schedule_test.rs`

### ET-WCN 冷却优化器
- `ETCoolingPolicy` / `ETWCNCooling` — 熵温度感知分区绑定
- `URXRuntime::set_et_policy()` — 替换默认分区绑定策略
- 在 `execute_graph()` 中透明接入，不影响其余调度流程

### 浮点指令集扩展
- 16 条 F64 opcode：UConstF64 / UAddF64 / USubF64 / UMulF64 / UDivF64 / USqrtF64 /
  UAbsF64 / UNegF64 / UFloorF64 / UCeilF64 / URoundF64 / UExpF64 / ULnF64 / UPowF64 /
  USinF64 / UCosinF64
- 类型转换：UI64ToF64 / UF64ToI64
- `tests/float_test.rs`：全浮点 opcode 单元测试

### 其他
- `tests/bench_test.rs`：CPU 顺序执行基准（~61K items/s）、JIT 编译速度基准（~17K/s）、GPU 大规模基准
- 依赖：`serialport = { version = "4", optional = true }`
- features：`gpu = [wgpu, bytemuck, pollster]`，`usb = [serialport]`

---

## v0.9

- **KDMapper FFI 集成**
  - Rust FFI 绑定层（kdmapper_ffi.rs）
  - C++ 包装库（kdmapper_wrapper.cpp/.hpp）
  - 动态加载支持（libloading）
  - Visual Studio + CMake 双构建支持
  - Mock 测试模式 + Native 生产模式
- 34 个测试全部通过
- 编译修复：MSVC v143 工具集适配、winioctl.h、ntdll.lib 链接

---

## v0.8

- Scheduler policy trait
- Topology-aware cost model
- Reservation / backfill skeleton

---

## v0.7

- Remote packet path
- Partition-level binding
- Reducer trait

---

## v0.6

- Rust 版 fusion + partition + inertia-aware reuse

---

## v0.5

- Rust 版 IRBlock → packet → ring → merge 端到端骨架

---

## v0.4

- Zero-copy 设计草案
- packet / header / payload / local ring 概念原型

---

## v0.3

- Block Fusion
- Graph Partition
- Inertia-aware Reuse

---

## v0.2

- IR Block
- 节点图执行
- 结果合并语义

---

## v0.1

- 最小指令表
- 多节点网络版调度器
