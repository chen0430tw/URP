# URP Runtime Zero-Copy 设计草案（参考 EasyTier 思路）

## 目标

把前面的 URP / URX 原型从“对象图 + Python dict 传递”推进到：

- **本地 ring bus**
- **header / payload 分离**
- **packet / block buffer 化**
- **节点间零拷贝风格传递**
- **merge 尽量在 view 上完成**

这版是 **Python 概念原型**，用于验证结构。  
真正高性能版更适合用 Rust 落地：

- `BytesMut / Bytes`
- `zerocopy`
- ring queue
- local fast path / remote packet path

---

## 1. 核心改动

### v0.3 之前
- IRBlock 是 Python 对象
- block outputs 是 dict
- 节点间传的是对象值

### v0.4 之后
- block 输出变成 **URPPacket**
- packet 底层是 `bytearray`
- header 用固定布局
- payload 用 `memoryview`
- 本地节点间通过 `LocalRingTunnel` 传递 packet
- merge 尽量直接看 payload view

---

## 2. 数据结构

### URPPacket
一个 packet 包含：

- `opcode`
- `merge_mode`
- `src_block`
- `dst_block`
- `payload_length`
- `payload_bytes`

概念上：

\[
Packet = Header + PayloadView
\]

### LocalRingTunnel
一个固定容量的环形通道：

- `push(packet)`
- `pop()`

用于：
- 同宿主节点
- 本地快路径
- 节点图内部流水线

---

## 3. 优化思想

### A. Buffer-first
先有 buffer，再有对象视图。  
不是先造很多对象，再序列化。

### B. Local fast path
同宿主节点优先走 ring，不走 socket 风格路径。

### C. Header-view merge
merge 不先 materialize 成 Python 大对象，而先按 header/payload view 处理。

### D. Inertia-buffer reuse
有相同 `inertia_key` 的块，优先复用相同 payload 形状和节点路径。

---

## 4. 当前版本边界

这版仍然是“结构验证版”，不做：

- 真正跨进程共享内存
- 真正网络包收发
- 真正 Rust zerocopy derive
- 真正 async runtime

它先证明：

> **URP 可以从对象流执行，推进到 packet / ring / view 风格执行。**
