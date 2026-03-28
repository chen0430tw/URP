//! WgpuExecutor — GPU compute backend via wgpu 29
//!
//! Enabled with `--features gpu`.
//!
//! GPU-accelerated opcodes (i32 arithmetic in WGSL):
//!   UAdd, USub, UMul, UDiv, URem, UCmpEq, UCmpLt, UCmpLe
//!   UAnd, UOr,  UXor, UNot, UShl, UShr,  UShra
//!
//! All other opcodes fall back to the CPU path (eval_opcode).

#[cfg(feature = "gpu")]
pub mod gpu {
    use std::collections::HashMap;
    use std::sync::Arc;

    use wgpu::util::DeviceExt;
    use wgpu::{
        BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
        BindingType, BufferBindingType, BufferDescriptor, BufferUsages,
        CommandEncoderDescriptor, ComputePassDescriptor, ComputePipelineDescriptor,
        DeviceDescriptor, Features, Limits, MapMode,
        PipelineLayoutDescriptor, PowerPreference, PollType, RequestAdapterOptions,
        ShaderModuleDescriptor, ShaderSource, ShaderStages,
    };

    use crate::executor::{eval_opcode, HardwareExecutor};
    use crate::ir::{IRBlock, Opcode};
    use crate::packet::PayloadValue;

    // ─────────────────────────────────────────────────────────────────────────
    // WgpuExecutor
    // ─────────────────────────────────────────────────────────────────────────

    pub struct WgpuExecutor {
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        /// Human-readable adapter description for diagnostics.
        pub adapter_info: String,
    }

    impl WgpuExecutor {
        /// Initialise the GPU executor.
        /// Returns `Err` if no suitable adapter is found or device creation fails.
        pub async fn new() -> Result<Self, String> {
            let instance = wgpu::Instance::default();

            let adapter = instance
                .request_adapter(&RequestAdapterOptions {
                    power_preference: PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                })
                .await
                .map_err(|e| format!("No GPU adapter found: {e}"))?;

            let info = adapter.get_info();
            let adapter_info = format!("{} ({:?})", info.name, info.backend);

            let (device, queue) = adapter
                .request_device(&DeviceDescriptor {
                    label: Some("URP-GPU"),
                    required_features: Features::empty(),
                    required_limits: Limits::default(),
                    ..Default::default()
                })
                .await
                .map_err(|e| format!("Device creation failed: {e}"))?;

            Ok(Self {
                device: Arc::new(device),
                queue: Arc::new(queue),
                adapter_info,
            })
        }

        // ─────────────────────────────────────────────────────────────────────
        // Core GPU dispatch
        // ─────────────────────────────────────────────────────────────────────

        /// Execute a binary i32 op on the GPU.
        /// `expr` is a WGSL expression using `a` and `b` (already loaded as i32).
        /// Example: `"a + b"`, `"a & b"`, `"select(0, 1, a == b)"`
        fn gpu_binop(&self, a: i64, b: i64, expr: &str) -> i64 {
            let shader_src = format!(r#"
@group(0) @binding(0) var<storage, read>       buf_a   : array<i32>;
@group(0) @binding(1) var<storage, read>       buf_b   : array<i32>;
@group(0) @binding(2) var<storage, read_write> buf_out : array<i32>;

@compute @workgroup_size(1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
    let a = buf_a[0];
    let b = buf_b[0];
    buf_out[0] = {expr};
}}
"#);
            self.run_shader(&shader_src, a, b)
        }

        /// Execute a unary i32 op on the GPU.
        /// `expr` uses `a` (loaded as i32); `b` input is ignored (passed as 0).
        fn gpu_unop(&self, a: i64, expr: &str) -> i64 {
            let shader_src = format!(r#"
@group(0) @binding(0) var<storage, read>       buf_a   : array<i32>;
@group(0) @binding(1) var<storage, read>       buf_b   : array<i32>;
@group(0) @binding(2) var<storage, read_write> buf_out : array<i32>;

@compute @workgroup_size(1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
    let a = buf_a[0];
    buf_out[0] = {expr};
}}
"#);
            self.run_shader(&shader_src, a, 0)
        }

        /// Upload two i32 inputs, compile & dispatch `shader_src`, read one i32 result.
        fn run_shader(&self, shader_src: &str, a: i64, b: i64) -> i64 {
            let device = &*self.device;
            let queue = &*self.queue;

            let shader = device.create_shader_module(ShaderModuleDescriptor {
                label: Some("urp_op"),
                source: ShaderSource::Wgsl(shader_src.into()),
            });

            let a_val: [i32; 1] = [a as i32];
            let b_val: [i32; 1] = [b as i32];

            let buf_a = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("a"),
                contents: bytemuck::cast_slice(&a_val),
                usage: BufferUsages::STORAGE,
            });
            let buf_b = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("b"),
                contents: bytemuck::cast_slice(&b_val),
                usage: BufferUsages::STORAGE,
            });
            let buf_out = device.create_buffer(&BufferDescriptor {
                label: Some("out"),
                size: 4,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let buf_readback = device.create_buffer(&BufferDescriptor {
                label: Some("readback"),
                size: 4,
                usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    storage_bgl_entry(0, true),
                    storage_bgl_entry(1, true),
                    storage_bgl_entry(2, false),
                ],
            });
            let bg = device.create_bind_group(&BindGroupDescriptor {
                label: None,
                layout: &bgl,
                entries: &[
                    BindGroupEntry { binding: 0, resource: buf_a.as_entire_binding() },
                    BindGroupEntry { binding: 1, resource: buf_b.as_entire_binding() },
                    BindGroupEntry { binding: 2, resource: buf_out.as_entire_binding() },
                ],
            });

            let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&bgl)],
                ..Default::default()
            });
            let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("urp_pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });

            let mut enc = device.create_command_encoder(&CommandEncoderDescriptor { label: None });
            {
                let mut pass = enc.begin_compute_pass(&ComputePassDescriptor {
                    label: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bg, &[]);
                pass.dispatch_workgroups(1, 1, 1);
            }
            enc.copy_buffer_to_buffer(&buf_out, 0, &buf_readback, 0, 4);
            queue.submit(Some(enc.finish()));

            let buf_slice = buf_readback.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            buf_slice.map_async(MapMode::Read, move |v| tx.send(v).unwrap());
            device.poll(PollType::wait_indefinitely()).expect("GPU poll failed");
            rx.recv().unwrap().unwrap();

            let mapped = buf_slice.get_mapped_range();
            let result: i32 = bytemuck::cast_slice::<u8, i32>(&mapped)[0];
            drop(mapped);
            buf_readback.unmap();

            result as i64
        }

        // ─────────────────────────────────────────────────────────────────────
        // Helpers: resolve i64 inputs from context
        // ─────────────────────────────────────────────────────────────────────

        fn i64_in<'a>(ctx: &'a HashMap<String, PayloadValue>, name: &str) -> i64 {
            match ctx.get(name).unwrap_or_else(|| panic!("missing input '{name}'")) {
                PayloadValue::I64(v) => *v,
                other => panic!("input '{name}' expected i64, got {other:?}"),
            }
        }

        fn ab(block: &IRBlock, ctx: &HashMap<String, PayloadValue>) -> (i64, i64) {
            (Self::i64_in(ctx, &block.inputs[0]), Self::i64_in(ctx, &block.inputs[1]))
        }
    }

    impl HardwareExecutor for WgpuExecutor {
        fn name(&self) -> &'static str { "wgpu" }

        fn exec(&self, block: &IRBlock, ctx: &HashMap<String, PayloadValue>) -> PayloadValue {
            match &block.opcode {
                // ── Batch 1: Arithmetic ───────────────────────────────────
                Opcode::UAdd => {
                    let (a, b) = Self::ab(block, ctx);
                    PayloadValue::I64(self.gpu_binop(a, b, "a + b"))
                }
                Opcode::USub => {
                    let (a, b) = Self::ab(block, ctx);
                    PayloadValue::I64(self.gpu_binop(a, b, "a - b"))
                }
                Opcode::UMul => {
                    let (a, b) = Self::ab(block, ctx);
                    PayloadValue::I64(self.gpu_binop(a, b, "a * b"))
                }
                Opcode::UDiv => {
                    let (a, b) = Self::ab(block, ctx);
                    assert!(b != 0, "UDiv: division by zero");
                    PayloadValue::I64(self.gpu_binop(a, b, "a / b"))
                }
                Opcode::URem => {
                    let (a, b) = Self::ab(block, ctx);
                    assert!(b != 0, "URem: division by zero");
                    PayloadValue::I64(self.gpu_binop(a, b, "a % b"))
                }

                // ── Batch 1: Comparison ───────────────────────────────────
                // WGSL `select(false_val, true_val, cond)` → returns i32 0 or 1
                Opcode::UCmpEq => {
                    let (a, b) = Self::ab(block, ctx);
                    PayloadValue::I64(self.gpu_binop(a, b, "select(0, 1, a == b)"))
                }
                Opcode::UCmpLt => {
                    let (a, b) = Self::ab(block, ctx);
                    PayloadValue::I64(self.gpu_binop(a, b, "select(0, 1, a < b)"))
                }
                Opcode::UCmpLe => {
                    let (a, b) = Self::ab(block, ctx);
                    PayloadValue::I64(self.gpu_binop(a, b, "select(0, 1, a <= b)"))
                }

                // ── Batch 2: Logic ────────────────────────────────────────
                Opcode::UAnd => {
                    let (a, b) = Self::ab(block, ctx);
                    PayloadValue::I64(self.gpu_binop(a, b, "a & b"))
                }
                Opcode::UOr => {
                    let (a, b) = Self::ab(block, ctx);
                    PayloadValue::I64(self.gpu_binop(a, b, "a | b"))
                }
                Opcode::UXor => {
                    let (a, b) = Self::ab(block, ctx);
                    PayloadValue::I64(self.gpu_binop(a, b, "a ^ b"))
                }
                Opcode::UNot => {
                    let a = Self::i64_in(ctx, &block.inputs[0]);
                    PayloadValue::I64(self.gpu_unop(a, "~a"))
                }

                // ── Batch 2: Shift ────────────────────────────────────────
                Opcode::UShl => {
                    let (a, b) = Self::ab(block, ctx);
                    // WGSL shift amount must be u32
                    PayloadValue::I64(self.gpu_binop(a, b,
                        "a << (u32(b) & 31u)"))
                }
                Opcode::UShr => {
                    // logical right shift: treat as u32
                    let (a, b) = Self::ab(block, ctx);
                    PayloadValue::I64(self.gpu_binop(a, b,
                        "i32(u32(a) >> (u32(b) & 31u))"))
                }
                Opcode::UShra => {
                    // arithmetic right shift: i32 preserves sign
                    let (a, b) = Self::ab(block, ctx);
                    PayloadValue::I64(self.gpu_binop(a, b,
                        "a >> (u32(b) & 31u)"))
                }

                // All other opcodes fall back to CPU
                _ => eval_opcode(block, ctx),
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Helpers
    // ─────────────────────────────────────────────────────────────────────────

    fn storage_bgl_entry(binding: u32, read_only: bool) -> BindGroupLayoutEntry {
        BindGroupLayoutEntry {
            binding,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }
    }
}

#[cfg(feature = "gpu")]
pub use gpu::WgpuExecutor;
