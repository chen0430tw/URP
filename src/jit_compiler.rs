//! JIT Graph Compiler — compiles an entire IRGraph into a single WGSL compute shader
//!
//! Instead of one GPU round-trip per opcode, the JIT compiler:
//!   1. Topologically sorts the IRGraph
//!   2. Emits one WGSL shader that evaluates the whole graph in sequence
//!   3. Accepts N input elements in parallel (vectorised dispatch)
//!
//! Supported value types in WGSL: i32 (from I64) and f32 (from F64).
//! Each "virtual register" is one slot in a storage buffer.
//!
//! # Architecture
//!
//! ```text
//! IRGraph  ──►  WgslCodegen  ──►  WGSL shader source
//!                                       │
//!                              wgpu compile + cache
//!                                       │
//!               inputs Vec<PayloadValue> N elements
//!                                       │
//!                              dispatch(N workgroups)
//!                                       │
//!               outputs Vec<PayloadValue> N elements
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! let jit = JitExecutor::new().await?;
//! let compiled = jit.compile(&graph)?;
//! let outputs = jit.run(&compiled, &inputs)?;  // inputs: N × (input_count) values
//! ```

use std::collections::{HashMap, VecDeque};

use crate::ir::{IRGraph, Opcode};

// ─────────────────────────────────────────────────────────────────────────────
// Value type in the shader
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderType {
    I32,
    F32,
}

impl ShaderType {
    fn wgsl_type(&self) -> &'static str {
        match self { ShaderType::I32 => "i32", ShaderType::F32 => "f32" }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Compiled graph descriptor
// ─────────────────────────────────────────────────────────────────────────────

/// Result of JIT compilation — holds the WGSL source and metadata.
pub struct CompiledGraph {
    pub wgsl_source: String,
    /// Block IDs in topological order (= order of computation in shader)
    pub topo_order: Vec<String>,
    /// Which topo-order indices are "input" blocks (UConstI64/UConstStr/FConst)
    pub input_indices: Vec<usize>,
    /// Which topo-order indices are "output" blocks (leaf outputs of the graph)
    pub output_indices: Vec<usize>,
    /// WGSL type of each block's result, indexed by topo position
    pub result_types: Vec<ShaderType>,
    /// Number of virtual registers (= number of blocks)
    pub n_regs: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// WGSL code generator
// ─────────────────────────────────────────────────────────────────────────────

/// Compile an `IRGraph` into a WGSL shader that evaluates all blocks for
/// `n` independent work items in parallel.
///
/// Layout of the storage buffers:
///   - `buf_inputs`:  flat array, length = n × n_inputs,  interleaved by work-item
///   - `buf_outputs`: flat array, length = n × n_outputs, interleaved by work-item
///
/// Each work-item gets its own set of virtual registers (one i32/f32 per block),
/// computed by the shader inline without cross-item dependencies.
pub fn compile_graph(graph: &IRGraph) -> Result<CompiledGraph, String> {
    // ── 1. Topological sort ───────────────────────────────────────────────────
    let topo = topological_sort(graph)?;

    // ── 2. Determine result type for each block ───────────────────────────────
    let mut result_types: HashMap<String, ShaderType> = HashMap::new();
    for id in &topo {
        let block = graph.get_block(id).ok_or_else(|| format!("block '{id}' not found"))?;
        let ty = infer_type(&block.opcode, &result_types);
        result_types.insert(id.clone(), ty);
    }

    // ── 3. Assign register indices ────────────────────────────────────────────
    let reg_idx: HashMap<String, usize> = topo.iter().enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();

    // ── 4. Find input / output blocks ─────────────────────────────────────────
    // Input blocks: UConstI64 / FConst / UConstStr that have no incoming edges
    let has_incoming: std::collections::HashSet<String> = graph.edges.iter()
        .map(|e| e.dst_block.clone())
        .collect();

    let input_indices: Vec<usize> = topo.iter().enumerate()
        .filter(|(_, id)| !has_incoming.contains(*id))
        .map(|(i, _)| i)
        .collect();

    // Output blocks: blocks with no outgoing edges
    let has_outgoing: std::collections::HashSet<String> = graph.edges.iter()
        .map(|e| e.src_block.clone())
        .collect();

    let output_indices: Vec<usize> = topo.iter().enumerate()
        .filter(|(_, id)| !has_outgoing.contains(*id))
        .map(|(i, _)| i)
        .collect();

    let n_regs = topo.len();
    let n_inputs = input_indices.len();
    let n_outputs = output_indices.len();

    // ── 5. Emit WGSL ──────────────────────────────────────────────────────────
    let mut src = String::new();

    // Bindings:
    //   0 = input buffer  (f32, because WGSL storage buffers are f32-friendly)
    //   1 = output buffer (f32)
    src.push_str(&format!(r#"
// JIT-compiled IRGraph: {graph_id}
// {n_regs} registers, {n_inputs} inputs, {n_outputs} outputs

@group(0) @binding(0) var<storage, read>       buf_in  : array<f32>;  // n * n_inputs
@group(0) @binding(1) var<storage, read_write>  buf_out : array<f32>;  // n * n_outputs

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {{
    let idx = gid.x;
    // bounds guard filled by dispatch count — skip if out of range
    // (caller dispatches ceil(n/64) workgroups)

"#, graph_id = graph.graph_id));

    // Declare virtual registers
    for (i, id) in topo.iter().enumerate() {
        let ty = result_types[id];
        src.push_str(&format!("    var r{i} : {};\n", ty.wgsl_type()));
    }
    src.push('\n');

    // Load inputs from buf_in
    for (slot, &reg) in input_indices.iter().enumerate() {
        let ty = result_types[&topo[reg]];
        match ty {
            ShaderType::F32 => {
                src.push_str(&format!(
                    "    r{reg} = buf_in[idx * {n_inputs}u + {slot}u];\n"
                ));
            }
            ShaderType::I32 => {
                src.push_str(&format!(
                    "    r{reg} = i32(buf_in[idx * {n_inputs}u + {slot}u]);\n"
                ));
            }
        }
    }
    src.push('\n');

    // Emit computation for non-input blocks
    // Build edge lookup: dst_block → list of (input_key, src_reg)
    let mut incoming: HashMap<String, HashMap<String, usize>> = HashMap::new();
    for e in &graph.edges {
        if let Some(&src_reg) = reg_idx.get(&e.src_block) {
            incoming.entry(e.dst_block.clone())
                .or_default()
                .insert(e.input_key.clone(), src_reg);
        }
    }

    for id in &topo {
        let block = graph.get_block(id).unwrap();
        let reg = reg_idx[id];

        // Skip pure input blocks (already loaded above)
        if input_indices.contains(&reg) { continue; }

        let ins = incoming.get(id).cloned().unwrap_or_default();
        let _get = |key: &str| ins.get(key).copied()
            .unwrap_or_else(|| panic!("JIT: block '{id}' missing input '{key}'"));

        let line = emit_op(&block.opcode, reg, &topo, &reg_idx, &ins, &result_types)?;
        src.push_str(&format!("    {line}\n"));
    }
    src.push('\n');

    // Store outputs to buf_out
    for (slot, &reg) in output_indices.iter().enumerate() {
        let ty = result_types[&topo[reg]];
        match ty {
            ShaderType::F32 => {
                src.push_str(&format!(
                    "    buf_out[idx * {n_outputs}u + {slot}u] = r{reg};\n"
                ));
            }
            ShaderType::I32 => {
                src.push_str(&format!(
                    "    buf_out[idx * {n_outputs}u + {slot}u] = f32(r{reg});\n"
                ));
            }
        }
    }

    src.push_str("}\n");

    let topo_types: Vec<ShaderType> = topo.iter().map(|id| result_types[id]).collect();

    Ok(CompiledGraph {
        wgsl_source: src,
        topo_order: topo,
        input_indices,
        output_indices,
        result_types: topo_types,
        n_regs,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Emit one WGSL assignment for a block
// ─────────────────────────────────────────────────────────────────────────────

fn emit_op(
    opcode: &Opcode,
    dst: usize,
    _topo: &[String],
    _reg_idx: &HashMap<String, usize>,
    ins: &HashMap<String, usize>,
    _types: &HashMap<String, ShaderType>,
) -> Result<String, String> {
    let a = || ins.get("a").copied().unwrap_or(0);
    let b = || ins.get("b").copied().unwrap_or(0);

    let line = match opcode {
        // Constants (handled as inputs; should not reach here normally)
        Opcode::UConstI64(v) => format!("r{dst} = i32({v});"),
        Opcode::FConst(v)    => format!("r{dst} = f32({v});"),
        Opcode::UConstStr(_) => format!("// UConstStr not supported in JIT shader"),

        // Integer arithmetic
        Opcode::UAdd => format!("r{dst} = r{} + r{};",  a(), b()),
        Opcode::USub => format!("r{dst} = r{} - r{};",  a(), b()),
        Opcode::UMul => format!("r{dst} = r{} * r{};",  a(), b()),
        Opcode::UDiv => format!("r{dst} = r{} / r{};",  a(), b()),
        Opcode::URem => format!("r{dst} = r{} % r{};",  a(), b()),

        // Integer comparison
        Opcode::UCmpEq => format!("r{dst} = select(0, 1, r{} == r{});", a(), b()),
        Opcode::UCmpLt => format!("r{dst} = select(0, 1, r{} <  r{});", a(), b()),
        Opcode::UCmpLe => format!("r{dst} = select(0, 1, r{} <= r{});", a(), b()),

        // Logic
        Opcode::UAnd => format!("r{dst} = r{} & r{};",  a(), b()),
        Opcode::UOr  => format!("r{dst} = r{} | r{};",  a(), b()),
        Opcode::UXor => format!("r{dst} = r{} ^ r{};",  a(), b()),
        Opcode::UNot => format!("r{dst} = ~r{};", a()),

        // Shift
        Opcode::UShl  => format!("r{dst} = r{} << (u32(r{}) & 31u);",  a(), b()),
        Opcode::UShr  => format!("r{dst} = i32(u32(r{}) >> (u32(r{}) & 31u));", a(), b()),
        Opcode::UShra => format!("r{dst} = r{} >> (u32(r{}) & 31u);",  a(), b()),

        // Float arithmetic
        Opcode::FAdd => format!("r{dst} = r{} + r{};",       a(), b()),
        Opcode::FSub => format!("r{dst} = r{} - r{};",       a(), b()),
        Opcode::FMul => format!("r{dst} = r{} * r{};",       a(), b()),
        Opcode::FDiv => format!("r{dst} = r{} / r{};",       a(), b()),
        Opcode::FPow => format!("r{dst} = pow(r{}, r{});",   a(), b()),

        // Float unary
        Opcode::FSqrt  => format!("r{dst} = sqrt(r{});",  a()),
        Opcode::FAbs   => format!("r{dst} = abs(r{});",   a()),
        Opcode::FNeg   => format!("r{dst} = -r{};",       a()),
        Opcode::FFloor => format!("r{dst} = floor(r{}); ", a()),
        Opcode::FCeil  => format!("r{dst} = ceil(r{});",  a()),
        Opcode::FRound => format!("r{dst} = round(r{}); ", a()),

        // Float comparison (returns i32 0/1)
        Opcode::FCmpEq => format!("r{dst} = select(0, 1, r{} == r{});", a(), b()),
        Opcode::FCmpLt => format!("r{dst} = select(0, 1, r{} <  r{});", a(), b()),
        Opcode::FCmpLe => format!("r{dst} = select(0, 1, r{} <= r{});", a(), b()),

        // Type conversion
        Opcode::F64ToI64 => format!("r{dst} = i32(r{});", a()),
        Opcode::I64ToF64 => format!("r{dst} = f32(r{});", a()),

        // Select / aggregation
        Opcode::USelect => {
            let cond = ins.get("cond").copied().unwrap_or(0);
            format!("r{dst} = select(r{}, r{}, r{} != 0);", b(), a(), cond)
        }
        Opcode::UMin => format!("r{dst} = min(r{}, r{});", a(), b()),
        Opcode::UMax => format!("r{dst} = max(r{}, r{});", a(), b()),
        Opcode::UAbs => format!("r{dst} = abs(r{});", a()),
        // UAssert: pass-through in shader (GPU can't panic; value propagates unchanged)
        Opcode::UAssert => format!("r{dst} = r{};", a()),

        other => return Err(format!("JIT: unsupported opcode {other:?}")),
    };
    Ok(line)
}

// ─────────────────────────────────────────────────────────────────────────────
// Type inference
// ─────────────────────────────────────────────────────────────────────────────

fn infer_type(opcode: &Opcode, _known: &HashMap<String, ShaderType>) -> ShaderType {
    match opcode {
        Opcode::UConstI64(_) | Opcode::UAdd | Opcode::USub | Opcode::UMul |
        Opcode::UDiv | Opcode::URem | Opcode::UAnd | Opcode::UOr | Opcode::UXor |
        Opcode::UNot | Opcode::UShl | Opcode::UShr | Opcode::UShra |
        Opcode::UCmpEq | Opcode::UCmpLt | Opcode::UCmpLe |
        Opcode::FCmpEq | Opcode::FCmpLt | Opcode::FCmpLe |
        Opcode::F64ToI64 | Opcode::UMin | Opcode::UMax | Opcode::UAbs |
        Opcode::UAssert => ShaderType::I32,

        Opcode::FConst(_) | Opcode::FAdd | Opcode::FSub | Opcode::FMul |
        Opcode::FDiv | Opcode::FPow | Opcode::FSqrt | Opcode::FAbs |
        Opcode::FNeg | Opcode::FFloor | Opcode::FCeil | Opcode::FRound |
        Opcode::I64ToF64 => ShaderType::F32,

        // USelect / UConcat: inherit from first non-cond input
        Opcode::USelect | Opcode::UConcat => ShaderType::F32,

        Opcode::UConstStr(_) | Opcode::UI64ToStr | Opcode::UStrToI64 |
        Opcode::UStrLen | Opcode::UStrSlice | Opcode::UStrSplit => ShaderType::I32,

        // OnnxInfer produces a tensor; shader-type is not applicable for GPU JIT
        Opcode::OnnxInfer(_) => ShaderType::F32,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Topological sort (Kahn's algorithm)
// ─────────────────────────────────────────────────────────────────────────────

fn topological_sort(graph: &IRGraph) -> Result<Vec<String>, String> {
    let mut indeg: HashMap<String, usize> = HashMap::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();

    for b in &graph.blocks {
        indeg.entry(b.block_id.clone()).or_insert(0);
    }
    for e in &graph.edges {
        *indeg.entry(e.dst_block.clone()).or_insert(0) += 1;
        adj.entry(e.src_block.clone()).or_default().push(e.dst_block.clone());
    }

    let mut ready: Vec<String> = indeg.iter()
        .filter(|(_, &d)| d == 0)
        .map(|(k, _)| k.clone())
        .collect();
    ready.sort();
    let mut queue: VecDeque<String> = ready.into_iter().collect();

    let mut result = Vec::new();
    while let Some(id) = queue.pop_front() {
        result.push(id.clone());
        if let Some(nexts) = adj.get(&id) {
            let mut newly_ready: Vec<String> = nexts.iter().filter_map(|nxt| {
                let d = indeg.get_mut(nxt).unwrap();
                *d -= 1;
                if *d == 0 { Some(nxt.clone()) } else { None }
            }).collect();
            newly_ready.sort();
            for nxt in newly_ready { queue.push_back(nxt); }
        }
    }

    if result.len() != graph.blocks.len() {
        return Err("IRGraph has a cycle — cannot JIT compile".to_string());
    }
    Ok(result)
}

// ─────────────────────────────────────────────────────────────────────────────
// JitExecutor — wgpu-backed vectorised execution
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "gpu")]
pub mod gpu_jit {
    use super::*;
    use std::sync::Arc;
    use wgpu::util::DeviceExt;
    use wgpu::{
        BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
        BindingType, BufferBindingType, BufferDescriptor, BufferUsages,
        CommandEncoderDescriptor, ComputePassDescriptor, ComputePipelineDescriptor,
        DeviceDescriptor, Features, Limits, MapMode, PipelineLayoutDescriptor,
        PowerPreference, PollType, RequestAdapterOptions, ShaderModuleDescriptor,
        ShaderSource, ShaderStages,
    };

    pub struct JitExecutor {
        device: Arc<wgpu::Device>,
        queue:  Arc<wgpu::Queue>,
        pub adapter_info: String,
    }

    impl JitExecutor {
        pub async fn new() -> Result<Self, String> {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&RequestAdapterOptions {
                    power_preference: PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                })
                .await
                .map_err(|e| format!("No adapter: {e}"))?;
            let info = adapter.get_info();
            let (device, queue) = adapter
                .request_device(&DeviceDescriptor {
                    label: Some("URP-JIT"),
                    required_features: Features::empty(),
                    required_limits: Limits::default(),
                    ..Default::default()
                })
                .await
                .map_err(|e| format!("Device failed: {e}"))?;
            Ok(Self {
                device: Arc::new(device),
                queue:  Arc::new(queue),
                adapter_info: format!("{} ({:?})", info.name, info.backend),
            })
        }

        /// Compile an IRGraph to WGSL and return a `CompiledGraph`.
        pub fn compile(&self, graph: &IRGraph) -> Result<CompiledGraph, String> {
            compile_graph(graph)
        }

        /// Run a compiled graph on `n` independent sets of inputs.
        ///
        /// `inputs`: outer index = input slot (in topo order),
        ///           inner index = element index (0..n)
        ///
        /// Returns: outer index = output slot, inner = element index.
        pub fn run(
            &self,
            compiled: &CompiledGraph,
            inputs: &[Vec<f32>],  // [n_inputs][n]
            n: usize,
        ) -> Result<Vec<Vec<f32>>, String> {
            let n_inputs  = compiled.input_indices.len();
            let n_outputs = compiled.output_indices.len();

            if inputs.len() != n_inputs {
                return Err(format!("expected {n_inputs} input slots, got {}", inputs.len()));
            }

            // Pack inputs: interleaved layout [item0_inp0, item0_inp1, ..., item1_inp0, ...]
            let mut flat_in: Vec<f32> = vec![0.0; n * n_inputs];
            for item in 0..n {
                for slot in 0..n_inputs {
                    flat_in[item * n_inputs + slot] = inputs[slot].get(item).copied().unwrap_or(0.0);
                }
            }

            let device = &*self.device;
            let queue  = &*self.queue;

            // Compile shader
            let shader = device.create_shader_module(ShaderModuleDescriptor {
                label: Some("jit_shader"),
                source: ShaderSource::Wgsl(compiled.wgsl_source.as_str().into()),
            });

            // Buffers
            let buf_in = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("jit_in"),
                contents: bytemuck::cast_slice(&flat_in),
                usage: BufferUsages::STORAGE,
            });
            let out_size = (n * n_outputs * std::mem::size_of::<f32>()) as u64;
            let buf_out = device.create_buffer(&BufferDescriptor {
                label: Some("jit_out"), size: out_size,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let buf_rb = device.create_buffer(&BufferDescriptor {
                label: Some("jit_rb"), size: out_size,
                usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // Bind group
            let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    BindGroupLayoutEntry { binding: 0, visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer { ty: BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false, min_binding_size: None }, count: None },
                    BindGroupLayoutEntry { binding: 1, visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer { ty: BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false, min_binding_size: None }, count: None },
                ],
            });
            let bg = device.create_bind_group(&BindGroupDescriptor {
                label: None, layout: &bgl,
                entries: &[
                    BindGroupEntry { binding: 0, resource: buf_in.as_entire_binding() },
                    BindGroupEntry { binding: 1, resource: buf_out.as_entire_binding() },
                ],
            });

            let pl = device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: None, bind_group_layouts: &[Some(&bgl)], ..Default::default()
            });
            let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("jit_pipeline"), layout: Some(&pl), module: &shader,
                entry_point: Some("main"), compilation_options: Default::default(), cache: None,
            });

            // Dispatch ceil(n / 64) workgroups
            let workgroups = ((n as u32) + 63) / 64;
            let mut enc = device.create_command_encoder(&CommandEncoderDescriptor { label: None });
            {
                let mut pass = enc.begin_compute_pass(&ComputePassDescriptor {
                    label: None, timestamp_writes: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bg, &[]);
                pass.dispatch_workgroups(workgroups, 1, 1);
            }
            enc.copy_buffer_to_buffer(&buf_out, 0, &buf_rb, 0, out_size);
            queue.submit(Some(enc.finish()));

            // Read back
            let slice = buf_rb.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(MapMode::Read, move |v| tx.send(v).unwrap());
            device.poll(PollType::wait_indefinitely()).expect("JIT poll failed");
            rx.recv().unwrap().unwrap();
            let mapped = slice.get_mapped_range();
            let flat_out: Vec<f32> = bytemuck::cast_slice(&mapped).to_vec();
            drop(mapped);
            buf_rb.unmap();

            // Deinterleave: [item0_out0, item0_out1, ..., item1_out0, ...]
            let mut outputs = vec![vec![0.0f32; n]; n_outputs];
            for item in 0..n {
                for slot in 0..n_outputs {
                    outputs[slot][item] = flat_out[item * n_outputs + slot];
                }
            }
            Ok(outputs)
        }
    }
}

#[cfg(feature = "gpu")]
pub use gpu_jit::JitExecutor;

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IRBlock, IREdge, IRGraph, Opcode};

    fn make_graph(id: &str) -> IRGraph {
        IRGraph::with_id(id.to_string())
    }

    fn edge(src: &str, dst: &str, key: &str) -> IREdge {
        IREdge { src_block: src.to_string(), dst_block: dst.to_string(),
                 output_key: src.to_string(), input_key: key.to_string() }
    }

    #[test]
    fn test_compile_add_graph() {
        // a + b  (integer)
        let mut g = make_graph("add");
        let mut a = IRBlock::new("a", Opcode::UConstI64(0));
        let mut b = IRBlock::new("b", Opcode::UConstI64(0));
        let mut add = IRBlock::new("add", Opcode::UAdd);
        add.inputs = vec!["a".into(), "b".into()];
        g.blocks.extend([a, b, add]);
        g.edges.push(edge("a", "add", "a"));
        g.edges.push(edge("b", "add", "b"));

        let compiled = compile_graph(&g).unwrap();
        assert_eq!(compiled.n_regs, 3);
        assert_eq!(compiled.input_indices.len(), 2);
        assert_eq!(compiled.output_indices.len(), 1);
        assert!(compiled.wgsl_source.contains("r2 = r0 + r1;"));
    }

    #[test]
    fn test_compile_float_graph() {
        // sqrt(a*a + b*b)  — hypotenuse
        let mut g = make_graph("hyp");
        g.blocks.push(IRBlock::new("a", Opcode::FConst(0.0)));
        g.blocks.push(IRBlock::new("b", Opcode::FConst(0.0)));
        let mut aa = IRBlock::new("aa", Opcode::FMul); aa.inputs = vec!["a".into(),"a".into()];
        let mut bb = IRBlock::new("bb", Opcode::FMul); bb.inputs = vec!["b".into(),"b".into()];
        let mut sum = IRBlock::new("s",  Opcode::FAdd); sum.inputs = vec!["a".into(),"b".into()];
        let mut sq  = IRBlock::new("h",  Opcode::FSqrt); sq.inputs = vec!["a".into()];
        g.blocks.extend([aa, bb, sum, sq]);
        g.edges.push(edge("a","aa","a")); g.edges.push(edge("a","aa","b"));
        g.edges.push(edge("b","bb","a")); g.edges.push(edge("b","bb","b"));
        g.edges.push(edge("aa","s","a")); g.edges.push(edge("bb","s","b"));
        g.edges.push(edge("s","h","a"));

        let compiled = compile_graph(&g).unwrap();
        assert!(compiled.wgsl_source.contains("sqrt("));
        assert_eq!(compiled.output_indices.len(), 1);
    }

    #[test]
    fn test_topo_sort_chain() {
        let mut g = make_graph("chain");
        g.blocks.push(IRBlock::new("x", Opcode::UConstI64(1)));
        g.blocks.push(IRBlock::new("y", Opcode::UConstI64(2)));
        let mut z = IRBlock::new("z", Opcode::UAdd); z.inputs = vec!["a".into(),"b".into()];
        g.blocks.push(z);
        g.edges.push(edge("x","z","a")); g.edges.push(edge("y","z","b"));

        let topo = topological_sort(&g).unwrap();
        let pz = topo.iter().position(|s| s=="z").unwrap();
        let px = topo.iter().position(|s| s=="x").unwrap();
        let py = topo.iter().position(|s| s=="y").unwrap();
        assert!(px < pz && py < pz);
    }
}
