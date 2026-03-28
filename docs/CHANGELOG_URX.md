# URX Changelog

## v0.1
- 最小指令表
- 多节点网络版调度器

## v0.2
- IR Block
- 节点图执行
- 结果合并语义

## v0.3
- Block Fusion
- Graph Partition
- Inertia-aware Reuse

## v0.4
- Zero-copy 设计草案
- packet / header / payload / local ring 概念原型

## v0.5
- Rust 版 IRBlock -> packet -> ring -> merge 端到端骨架

## v0.6
- Rust 版 fusion + partition + inertia-aware reuse

## v0.7
- remote packet path
- partition-level binding
- reducer trait

## v0.8
- scheduler policy trait
- topology-aware cost model
- reservation / backfill skeleton

## v0.9
- **KDMapper FFI 集成** ✅
  - Rust FFI 绑定层 (kdmapper_ffi.rs)
  - C++ 包装库 (kdmapper_wrapper.{cpp,hpp})
  - 动态加载支持 (libloading)
  - Visual Studio + CMake 双构建支持
  - Mock 测试模式 + Native 生产模式
  - 34 个测试全部通过
- 编译修复:
  - MSVC v143 工具集适配
  - winioctl.h 头文件添加
  - ATL 依赖移除
  - ntdll.lib 链接
- 测试覆盖: 6 KDMapper 单元测试 + 28 集成测试
