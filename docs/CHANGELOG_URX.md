# URX Changelog

## v1.2（当前）

### 真实网络传输（RemotePacketLink TCP 实现）
- `RemotePacketLink::serve(addr, handler)` — tokio TCP 服务器，每连接独立 `tokio::spawn`，`PacketCodec` 帧封装（4 字节 BE 长度前缀）
- `runtime.rs` 远程路由修复：`dst_node.address` 有值时走真实 TCP（`remote.send(addr, packet)`），无地址时退回桩实现
- `Node::address: Option<String>` — 节点 TCP 端点字段（`with_address()` builder）
- 借用生命周期修复：`src_n`/`dst_n` 引用在 `.await` 前归还，`dst_addr` 提前 `.clone()` 解耦

### 真实 ONNX 模型推理
- `src/onnx_executor.rs`（新文件）：`OnnxExecutor::load(path)` 构建时预加载 `ort::Session`，`exec()` 遇 `OnnxInfer` 走真实推理，其余 opcode 委托 `eval_opcode`
- `Opcode::OnnxInfer(String)` — ONNX 路径 opcode（`ir.rs`）
- `PayloadValue::Tensor(Vec<f32>, Vec<usize>)` — 扁平行优先 f32 张量（`packet.rs`），`PayloadCodec` 支持 TYPE_TENSOR=5 编解码
- `IRGraph::from_onnx(path)` — 通过 `ort::Session` 自省输入名称，自动构建「占位输入 → 推理块」计算图
- `CpuExecutor` 遇 `OnnxInfer` 给出明确 panic 提示
- Cargo.toml：`ort = "2.0.0-rc.12"` + `ndarray = "0.16"`，feature `onnx = ["ort", "ndarray"]`

### 算法与传递逻辑全面优化

#### `scheduler.rs` — 并发执行 + O(E+B) DAG 构建
- `build_partition_dag`：O(P×B×E) → O(E+B)，一次扫全部 edges 建依赖图，替代对每个 partition 调用 `external_inputs()`
- `execute_dag_partitions`：串行 await → **真正并发**，用 `tokio::task::JoinSet` 同时 spawn 所有就绪 partition；任意一个完成即解锁后继，无 wave 级同步屏障
- `AsyncLane::semaphore_arc()` — 暴露 `Arc<Semaphore>` 供跨任务捕获
- `HardwareExecutor: 'static` — `JoinSet::spawn` 所需约束

#### `optimizer.rs` — 三处 O(1) 优化
| 原来 | 现在 |
|------|------|
| `graph.get_block(id)` O(B) 线性扫 | 预建 `block_map: HashMap<&str, &IRBlock>`，O(1) |
| `edges.iter().filter(src==a && dst==b)` O(E) 过滤 | 预建 `edge_count: HashMap<(&str,&str), usize>`，O(1) |
| `new_edges.iter().any(...)` 去重 O(E²) | `HashSet<(src,dst,out_key,in_key)>` 去重 O(E) |

#### `partition.rs` — O(B²) → O(B)
- `bind_partitions` 预建 `block_index: HashMap<&str, &IRBlock>`，`get_block` O(B) 扫描改为 O(1) 查找

#### `scheduler.rs`（`external_inputs` 局部优化）
- 改为先建 `my_blocks: HashSet`，再对 edges 过滤 dst，避免嵌套全量扫描

---

## v1.1

### PartitionDAGScheduler 完整集成
- `PartitionDAGScheduler` 从死代码升级为 `URXRuntime::execute_graph()` 的核心调度层
- `AsyncLane` 接口重构：从 block 级闭包 `Fn(IRBlock, String)` → partition 级 `Fn(Partition) -> Vec<BlockExecutionResult>`
- `execute_graph()` 将所有可变执行状态（inbox / packet_log / rings / remote / inertia）包装为 `Arc<Mutex<>>` 供 async 闭包捕获
- 删除冗余的 `topo_sort_partitions` 自由函数（调度器内部处理）
- 删除 `AsyncLane.id` 字段（未使用），`new()` 改为 `_id: String`

### IRGraph JSON 序列化
- 为 `MergeMode`、`Opcode`、`IRBlock`、`IREdge`、`IRGraph` 添加 `serde::{Serialize, Deserialize}` 派生
- `IRGraph::from_json(s)` / `to_json()` — 字符串级往返
- `IRGraph::load_json(path)` / `save_json(path)` — 文件级 I/O
- `tests/json_schema_test.rs`：`test_json_round_trip`、`test_json_schema_opcodes` 两个测试

### 新演示（Demo H–L）
- **Demo H** — ZeroCopyContext：SharedMemoryRegion 写/读/reader_count；BufferPool acquire/release/stats；InertiaBufferCache put/get/LRU 淘汰（容量=3）；ZeroCopyContext 统一门面
- **Demo I** — JIT 编译（CPU 路径）：4 块 FConst→FMul→FAdd 图，验证 CompiledGraph（n_regs=4，2 输入，1 输出），WGSL 源预览
- **Demo J** — USB 协议层：crc8、encode_request/decode_response 往返、STATUS 常量、CRC 错误检测、`BlockExecutor::exec(UAdd, {a:21,b:21})=42`、UsbLoopbackExecutor 和 UsbCpuFallbackExecutor 执行 √5≈2.236068
- **Demo K** — ReservationAwarePolicy：自定义 `EarliestSlotPolicy`，node0 占满 0–100 时路由 10 槽任务到 node1，High/Critical 优先级变体
- **Demo L** — 真实工作负载图：从 `C:\Users\asus\urp\` 加载 `fft_n64_s6.json`、`fft_n128_s7.json`、`attn_h4_s32.json`、`resnet_8blk_c64.json`，4 个 CPU 节点执行，报告统计

### 工作站 Agent 集成
- README 新增"工作站 Agent 操作指南"章节：JSON 格式规范、opcode 命名规则对照表、ReLU 正确写法、运行步骤
- 开发者接手说明更新：PartitionDAGScheduler 集成状态、JSON 图加载工作流、Agent 操作方式

### 死代码清理
- 所有模块统一 `#![allow(dead_code)]`（库 API 警告预期存在）
- `NodeType` 枚举添加 `#[allow(dead_code)]`
- 删除 `AsyncLane.id` 字段（唯一真正未使用的字段）

---

## v1.0

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
