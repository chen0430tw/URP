# URP + KDMapper 集成设计方案

> **版本**: v0.1.0
> **日期**: 2026-03-27
> **状态**: 设计阶段

---

## 📋 目录

- [项目概述](#项目概述)
- [背景](#背景)
- [架构设计](#架构设计)
- [模块设计](#模块设计)
- [实现计划](#实现计划)
- [风险评估](#风险评估)
- [法律声明](#法律声明)

---

## 项目概述

### 目标

将 **KDMapper** 内核驱动映射能力集成到 **URP (Universal Reconstructive Processor)** 分布式运行时中，实现：

1. **分布式内核任务调度** - 在多台机器上并行执行内核操作
2. **安全的研究框架** - 为授权的安全研究提供可扩展平台
3. **统一的任务抽象** - 将用户态和内核态操作统一到 DAG 调度模型

### 非目标

- 游戏作弊工具
- 恶意软件平台
- 未授权系统访问

---

## 背景

### KDMapper 简介

| 特性 | 描述 |
|------|------|
| **核心功能** | 绕过 Windows 驱动签名，手动映射未签名驱动到内核 |
| **利用方式** | 使用 Intel 脆弱驱动 `iqvw64e.sys` (Hao et al. 2018) |
| **技术流程** | 内存分配 → 节区复制 → 重定位修复 → Shellcode 执行 |
| **支持版本** | Windows 10 build 1809, 1903, 1909, 2004 |
| **语言** | C/C++ |

### URP 简介

URP 是一个基于 DAG 的分布式运行时，具备：

- **分区 DAG 调度** - 拓扑感知的并行执行
- **零拷贝共享内存** - 高效跨节点数据传输
- **远程数据包路由** - Local Ring 快速路径 + 远程回退
- **预约/回填系统** - 时间感知的资源调度

---

## 架构设计

### 整体架构

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Application Layer                              │
│                     (Security Research / Kernel Debugging)              │
└─────────────────────────────────────────┬───────────────────────────────┘
                                          │
┌─────────────────────────────────────────▼───────────────────────────────┐
│                              URP Runtime                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐ │
│  │   DAG        │  │   Partition  │  │  Zero-Copy   │  │   Remote     │ │
│  │  Scheduler   │  │   Binding    │  │   Shared     │  │   Routing    │ │
│  │              │  │              │  │   Memory     │  │              │ │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘ │
└─────────┼──────────────────┼──────────────────┼──────────────────┼───────┘
          │                  │                  │                  │
    ┌─────▼─────┐      ┌────▼────┐      ┌─────▼─────┐      ┌─────▼─────┐
    │  CPU      │      │  GPU     │      │  Network  │      │  KDMapper  │
    │  Node     │      │  Node    │      │  Node     │      │   Node     │
    │ (User)    │      │ (User)   │      │  (User)   │      │ (Kernel)   │
    └───────────┘      └──────────┘      └───────────┘      └─────┬───────┘
                                                                   │
                                                    ┌──────────────▼───────────┐
                                                    │   Intel Vulnerable       │
                                                    │   Driver (iqvw64e.sys)   │
                                                    │   + Target Driver        │
                                                    └──────────────────────────┘
```

### 分层设计

```
┌─────────────────────────────────────────────────────────────────────┐
│ Layer 4: Application API                                            │
│ - Research Task API                                                 │
│ - Kernel Operation DSL                                              │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│ Layer 3: URP Runtime + KDMapper Integration                          │
│ - Kernel-aware Scheduler Policy                                     │
│ - Kernel Operation Blocks                                           │
│ - Permission Boundary                                               │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│ Layer 2: KDMapper FFI Layer                                          │
│ - Rust-C++ Interop                                                  │
│ - Driver Mapping Interface                                          │
│ - Memory Access Interface                                           │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│ Layer 1: KDMapper Core                                               │
│ - Intel Driver Loading                                              │
│ - Memory Allocation                                                  │
│ - Shellcode Execution                                               │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 模块设计

### 1. IR 扩展 - 内核操作码

```rust
// src/ir.rs - 扩展 Opcode 枚举

#[derive(Debug, Clone)]
pub enum Opcode {
    // ===== 现有用户态操作码 =====
    UConstI64(i64),
    UConstStr(String),
    UAdd,
    UConcat,

    // ===== 新增内核操作码 =====

    /// 加载内核驱动
    UKernelDriverLoad {
        driver_path: String,
        init_shellcode: Vec<u8>,
        flags: DriverLoadFlags,
    },

    /// 卸载内核驱动
    UKernelDriverUnload {
        driver_name: String,
    },

    /// 内核内存读取
    UKernelMemoryRead {
        address: u64,
        size: usize,
    },

    /// 内核内存写入
    UKernelMemoryWrite {
        address: u64,
        data: Vec<u8>,
    },

    /// 内核 Shellcode 执行
    UKernelShellcodeExec {
        shellcode: Vec<u8>,
        timeout_ms: u32,
    },

    /// 获取内核模块基址
    UKernelGetModuleBase {
        module_name: String,
    },

    /// 内核模式函数调用
    UKernelCallFunction {
        address: u64,
        args: Vec<u64>,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct DriverLoadFlags {
    pub ignore_signature: bool,
    pub erase_pe_headers: bool,
    pub manual_map: bool,
}

impl IRBlock {
    /// 检查此 block 是否需要内核权限
    pub fn requires_kernel_mode(&self) -> bool {
        matches!(self.opcode,
            Opcode::UKernelDriverLoad { .. }
            | Opcode::UKernelDriverUnload { .. }
            | Opcode::UKernelMemoryRead { .. }
            | Opcode::UKernelMemoryWrite { .. }
            | Opcode::UKernelShellcodeExec { .. }
            | Opcode::UKernelGetModuleBase { .. }
            | Opcode::UKernelCallFunction { .. }
        )
    }
}
```

### 2. 节点类型扩展

```rust
// src/node.rs - 扩展 NodeType

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeType {
    Cpu,
    Gpu,
    Qcu,
    Memory,
    Network,
    Rule,
    Structure,
    Kernel,  // 新增：具有内核权限的节点
}

impl Node {
    /// 检查节点是否具有内核执行能力
    pub fn has_kernel_capability(&self) -> bool {
        self.node_type == NodeType::Kernel
    }

    /// 获取内核驱动路径
    pub fn kernel_driver_path(&self) -> Option<&str> {
        if self.has_kernel_capability() {
            Some(self.kernel_driver.as_str())
        } else {
            None
        }
    }
}
```

### 3. KDMapper FFI 模块

```rust
// src/kdmapper.rs - 新增模块

//! KDMapper FFI Binding Layer
//!
//! This module provides safe Rust bindings to KDMapper functionality.
//! It acts as a bridge between URP runtime and the KDMapper C++ core.

use std::path::Path;
use std::ffi::CString;

/// KDMapper 错误类型
#[derive(Debug, Clone)]
pub enum KDMapperError {
    DriverLoadFailed,
    MemoryAllocationFailed,
    ShellcodeExecutionFailed,
    InvalidAddress,
    PermissionDenied,
    Timeout,
    Unknown(String),
}

/// KDMapper 结果类型
pub type Result<T> = std::result::Result<T, KDMapperError>;

/// 驱动映射配置
#[derive(Debug, Clone)]
pub struct DriverMappingConfig {
    /// Intel 脆弱驱动路径
    pub intel_driver_path: String,

    /// 目标驱动路径
    pub target_driver_path: String,

    /// 初始化 Shellcode
    pub init_shellcode: Option<Vec<u8>>,

    /// 是否清除 PE 头
    pub erase_headers: bool,

    /// 超时时间（毫秒）
    pub timeout_ms: u32,
}

impl Default for DriverMappingConfig {
    fn default() -> Self {
        Self {
            intel_driver_path: "iqvw64e.sys".to_string(),
            target_driver_path: String::new(),
            init_shellcode: None,
            erase_headers: true,
            timeout_ms: 5000,
        }
    }
}

/// 驱动映射结果
#[derive(Debug, Clone)]
pub struct DriverMappingResult {
    /// 驱动基址
    pub base_address: u64,

    /// 驱动大小
    pub size: usize,

    /// 入口点地址
    pub entry_point: u64,

    /// 执行状态
    pub success: bool,
}

/// KDMapper 执行器
pub struct KDMapperExecutor {
    intel_driver_loaded: bool,
    loaded_drivers: Vec<String>,
}

impl KDMapperExecutor {
    /// 创建新的 KDMapper 执行器
    pub fn new() -> Self {
        Self {
            intel_driver_loaded: false,
            loaded_drivers: Vec::new(),
        }
    }

    /// 映射驱动到内核
    pub async fn map_driver(&mut self, config: DriverMappingConfig) -> Result<DriverMappingResult> {
        // 1. 确保 Intel 驱动已加载
        if !self.intel_driver_loaded {
            self.load_intel_driver(&config.intel_driver_path).await?;
            self.intel_driver_loaded = true;
        }

        // 2. 读取目标驱动文件
        let driver_data = tokio::fs::read(&config.target_driver_path)
            .await
            .map_err(|e| KDMapperError::Unknown(e.to_string()))?;

        // 3. 解析 PE 结构
        let pe_info = self.parse_pe(&driver_data)?;

        // 4. 分配内核内存
        let base_address = self.allocate_kernel_memory(pe_info.image_size).await?;

        // 5. 复制节区到内核内存
        self.copy_sections_to_kernel(base_address, &driver_data, &pe_info).await?;

        // 6. 修复重定位
        self.fix_relocations(base_address, &pe_info).await?;

        // 7. 解析导入表（如果有）
        self.resolve_imports(base_address, &pe_info).await?;

        // 8. 执行初始化代码
        if let Some(shellcode) = config.init_shellcode {
            self.execute_shellcode(&shellcode, config.timeout_ms).await?;
        } else {
            self.execute_driver_entry(base_address, pe_info.entry_point).await?;
        }

        // 9. 可选：清除 PE 头
        if config.erase_headers {
            self.erase_pe_headers(base_address, pe_info.headers_size).await?;
        }

        Ok(DriverMappingResult {
            base_address,
            size: pe_info.image_size,
            entry_point: base_address + pe_info.entry_point,
            success: true,
        })
    }

    /// 读取内核内存
    pub async fn read_kernel_memory(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        // 通过 Intel 驱动读取内核内存
        todo!("Implement via FFI to KDMapper")
    }

    /// 写入内核内存
    pub async fn write_kernel_memory(&self, address: u64, data: &[u8]) -> Result<()> {
        // 通过 Intel 驱动写入内核内存
        todo!("Implement via FFI to KDMapper")
    }

    /// 执行 Shellcode
    pub async fn execute_shellcode(&self, shellcode: &[u8], timeout_ms: u32) -> Result<u64> {
        // 在内核上下文中执行 shellcode
        todo!("Implement via FFI to KDMapper")
    }

    /// 获取模块基址
    pub async fn get_module_base(&self, module_name: &str) -> Result<u64> {
        // 查询内核模块基址
        todo!("Implement via FFI to KDMapper")
    }

    // ===== 私有辅助方法 =====

    async fn load_intel_driver(&mut self, path: &str) -> Result<()> {
        // 使用 Windows SCM 加载 Intel 驱动
        todo!("Implement via FFI to KDMapper")
    }

    fn parse_pe(&self, data: &[u8]) -> Result<PEInfo> {
        // 解析 PE 文件结构
        todo!("Implement PE parsing")
    }

    async fn allocate_kernel_memory(&self, size: usize) -> Result<u64> {
        todo!("Implement via FFI to KDMapper")
    }

    async fn copy_sections_to_kernel(&self, base: u64, data: &[u8], pe: &PEInfo) -> Result<()> {
        todo!("Implement via FFI to KDMapper")
    }

    async fn fix_relocations(&self, base: u64, pe: &PEInfo) -> Result<()> {
        todo!("Implement via FFI to KDMapper")
    }

    async fn resolve_imports(&self, base: u64, pe: &PEInfo) -> Result<()> {
        todo!("Implement via FFI to KDMapper")
    }

    async fn execute_driver_entry(&self, base: u64, entry: u32) -> Result<()> {
        todo!("Implement via FFI to KDMapper")
    }

    async fn erase_pe_headers(&self, base: u64, size: usize) -> Result<()> {
        todo!("Implement via FFI to KDMapper")
    }
}

/// PE 文件信息
#[derive(Debug, Clone)]
struct PEInfo {
    image_size: usize,
    entry_point: u32,
    headers_size: usize,
    sections: Vec<SectionInfo>,
}

#[derive(Debug, Clone)]
struct SectionInfo {
    name: String,
    virtual_address: u32,
    size: u32,
    characteristics: u32,
}

impl Default for KDMapperExecutor {
    fn default() -> Self {
        Self::new()
    }
}
```

### 4. 内核感知调度策略

```rust
// src/kernel_policy.rs - 新增模块

//! Kernel-aware scheduling policy for URP
//!
//! This policy ensures that kernel-mode blocks are only scheduled
//! on nodes with kernel execution capabilities.

use crate::node::Node;
use crate::policy::SchedulerPolicy;
use std::collections::{HashMap, HashSet};

/// 内核感知调度策略
pub struct KernelAwarePolicy {
    base_policy: MultifactorPolicy,
    allow_kernel_fallback: bool,
}

impl KernelAwarePolicy {
    pub fn new() -> Self {
        Self {
            base_policy: MultifactorPolicy::new(),
            allow_kernel_fallback: false,
        }
    }

    /// 设置是否允许内核节点回退到用户态
    pub fn with_kernel_fallback(mut self, allow: bool) -> Self {
        self.allow_kernel_fallback = allow;
        self
    }
}

impl SchedulerPolicy for KernelAwarePolicy {
    fn select_partition_node(
        &self,
        required_tags: &HashSet<String>,
        preferred_zone: &str,
        inertia_key: Option<&str>,
        nodes: &HashMap<String, Node>,
    ) -> String {
        // 检查是否需要内核权限
        let requires_kernel = required_tags.contains("kernel");

        // 过滤可用节点
        let mut candidates: Vec<_> = nodes.values()
            .filter(|n| {
                if requires_kernel {
                    n.has_kernel_capability()
                } else {
                    true
                }
            })
            .collect();

        if candidates.is_empty() && requires_kernel && self.allow_kernel_fallback {
            // 回退：允许在用户态节点上执行（会失败）
            candidates = nodes.values().collect();
        }

        if candidates.is_empty() {
            panic!("No suitable node for partition");
        }

        // 使用基础策略评分
        candidates
            .into_iter()
            .map(|n| {
                let mut score = 0.0f32;
                for tag in required_tags {
                    score += node_score(tag, preferred_zone, inertia_key, n);
                }
                // 内核节点优先
                if n.has_kernel_capability() && requires_kernel {
                    score += 1000.0;
                }
                (score, n.node_id.clone())
            })
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .expect("no node selected")
            .1
    }
}
```

### 5. 内核操作执行器

```rust
// src/kernel_executor.rs - 新增模块

//! Kernel operation executor for URP
//!
//! Executes kernel-mode blocks through KDMapper interface.

use crate::ir::{IRBlock, Opcode};
use crate::kdmapper::KDMapperExecutor;
use crate::packet::PayloadValue;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 内核操作执行器
pub struct KernelBlockExecutor {
    kdmapper: Arc<Mutex<KDMapperExecutor>>,
    loaded_drivers: HashMap<String, u64>, // name -> base_address
}

impl KernelBlockExecutor {
    pub fn new(kdmapper: KDMapperExecutor) -> Self {
        Self {
            kdmapper: Arc::new(Mutex::new(kdmapper)),
            loaded_drivers: HashMap::new(),
        }
    }

    /// 执行内核操作块
    pub async fn exec_kernel_block(
        &mut self,
        block: &IRBlock,
        ctx: &HashMap<String, PayloadValue>,
    ) -> PayloadValue {
        match &block.opcode {
            Opcode::UKernelDriverLoad { driver_path, init_shellcode, flags } => {
                let config = crate::kdmapper::DriverMappingConfig {
                    intel_driver_path: "iqvw64e.sys".to_string(),
                    target_driver_path: driver_path.clone(),
                    init_shellcode: init_shellcode.clone(),
                    erase_headers: flags.erase_pe_headers,
                    timeout_ms: 5000,
                };

                let mut kdmapper = self.kdmapper.lock().await;
                match kdmapper.map_driver(config).await {
                    Ok(result) => {
                        self.loaded_drivers.insert(driver_path.clone(), result.base_address);
                        PayloadValue::I64(result.base_address as i64)
                    }
                    Err(e) => {
                        PayloadValue::Str(format!("ERROR: {:?}", e))
                    }
                }
            }

            Opcode::UKernelMemoryRead { address, size } => {
                let kdmapper = self.kdmapper.lock().await;
                match kdmapper.read_kernel_memory(*address, *size).await {
                    Ok(data) => {
                        // 将字节数组转换为 hex 字符串
                        PayloadValue::Str(hex::encode(data))
                    }
                    Err(e) => PayloadValue::Str(format!("ERROR: {:?}", e)),
                }
            }

            Opcode::UKernelMemoryWrite { address, data } => {
                let kdmapper = self.kdmapper.lock().await;
                match kdmapper.write_kernel_memory(*address, data).await {
                    Ok(()) => PayloadValue::I64(0),
                    Err(e) => PayloadValue::Str(format!("ERROR: {:?}", e)),
                }
            }

            Opcode::UKernelShellcodeExec { shellcode, timeout_ms } => {
                let kdmapper = self.kdmapper.lock().await;
                match kdmapper.execute_shellcode(shellcode, *timeout_ms).await {
                    Ok(result) => PayloadValue::I64(result as i64),
                    Err(e) => PayloadValue::Str(format!("ERROR: {:?}", e)),
                }
            }

            Opcode::UKernelGetModuleBase { module_name } => {
                let kdmapper = self.kdmapper.lock().await;
                match kdmapper.get_module_base(module_name).await {
                    Ok(base) => PayloadValue::I64(base as i64),
                    Err(e) => PayloadValue::Str(format!("ERROR: {:?}", e)),
                }
            }

            _ => PayloadValue::Str("NOT_A_KERNEL_OP".to_string()),
        }
    }
}
```

---

## 实现计划

### 阶段 1: 基础设施 (1-2 周)

| 任务 | 描述 | 优先级 |
|------|------|--------|
| IR 扩展 | 添加内核操作码到 `Opcode` 枚举 | P0 |
| 节点类型 | 添加 `NodeType::Kernel` | P0 |
| KDMapper FFI | 创建基础 FFI 绑定结构 | P0 |
| 文档更新 | 更新 API 文档 | P1 |

### 阶段 2: 核心功能 (2-3 周)

| 任务 | 描述 | 优先级 |
|------|------|--------|
| 驱动加载 | 实现 `map_driver` 功能 | P0 |
| 内存访问 | 实现内核读写接口 | P0 |
| Shellcode 执行 | 实现安全执行机制 | P0 |
| 错误处理 | 完善错误类型和传播 | P1 |

### 阶段 3: 集成 (1-2 周)

| 任务 | 描述 | 优先级 |
|------|------|--------|
| 调度策略 | 实现内核感知调度 | P0 |
| 执行器集成 | 集成到 BlockExecutor | P0 |
| 测试套件 | 添加单元测试和集成测试 | P0 |

### 阶段 4: 优化 (1 周)

| 任务 | 描述 | 优先级 |
|------|------|--------|
| 性能优化 | 减少 FFI 开销 | P1 |
| 安全加固 | 添加权限检查 | P0 |
| 日志记录 | 添加详细审计日志 | P1 |

---

## 文件结构

```
URP/
├── src/
│   ├── ir.rs              # 扩展 Opcode
│   ├── node.rs            # 扩展 NodeType
│   ├── kdmapper.rs        # 新增：KDMapper FFI
│   ├── kernel_policy.rs   # 新增：内核感知调度
│   ├── kernel_executor.rs # 新增：内核操作执行器
│   └── ...
├── kdmapper/              # 新增：KDMapper C++ 子模块
│   ├── include/
│   │   └── kdmapper.hpp
│   ├── src/
│   │   ├── driver.cpp
│   │   ├── memory.cpp
│   │   └── shellcode.cpp
│   ├── CMakeLists.txt
│   └── README.md
├── tests/
│   ├── kdmapper_test.rs   # 新增：KDMapper 测试
│   └── ...
└── docs/
    └── KDMAPPER_INTEGRATION.md
```

---

## 风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| **法律风险** | 高 | 仅用于授权研究，添加许可声明 |
| **安全风险** | 高 | 权限隔离，审计日志 |
| **稳定性风险** | 中 | 全面测试，错误恢复 |
| **维护风险** | 中 | 文档完善，模块化设计 |
| **依赖风险** | 低 | KDMapper 是成熟项目 |

---

## 安全考虑

### 权限隔离

```rust
/// 权限检查
pub fn check_kernel_permission(node: &Node) -> bool {
    // 1. 检查节点类型
    if !node.has_kernel_capability() {
        return false;
    }

    // 2. 检查授权状态
    if !node.is_authorized() {
        return false;
    }

    // 3. 检查操作合法性
    // ...

    true
}
```

### 审计日志

```rust
/// 内核操作审计日志
#[derive(Debug, Clone)]
pub struct KernelOperationLog {
    pub timestamp: u64,
    pub node_id: String,
    pub operation: String,
    pub parameters: String,
    pub result: String,
    pub authorized: bool,
}
```

---

## 法律声明

**本设计文档仅供教育和安全研究目的。**

- ❌ **禁止用于**：游戏作弊、恶意软件、未授权访问
- ✅ **允许用于**：授权的安全研究、内核调试、教育学习

**使用者需自行承担所有法律责任。**

---

## 参考资料

- [Dark7oveRR/kdmapper](https://github.com/Dark7oveRR/kdmapper)
- [TheCruZ/kdmapper](https://github.com/TheCruZ/kdmapper)
- [KDMapper驱动映射工具完全指南](https://blog.csdn.net/gitblog_00401/article/details/155223144)
- [Intel Driver Vulnerability (Hao et al. 2018)](https://github.com/Sticky-Rootkit/Intel-Driver)

---

**文档版本历史**

| 版本 | 日期 | 变更 |
|------|------|------|
| v0.1.0 | 2026-03-27 | 初始设计 |
