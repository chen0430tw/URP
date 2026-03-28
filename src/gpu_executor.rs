//! WgpuExecutor — GPU compute backend via wgpu 29
//!
//! Enabled with `--features gpu`.
//!
//! Supported opcodes on GPU:
//!   UAdd  — two i64 inputs added via WGSL compute shader (cast to i32 in shader,
//!            covers values up to ±2^31 − 1)
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
            // wgpu 29: Instance::default() calls new_without_display_handle() internally
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

            // wgpu 29: request_device takes only &DeviceDescriptor (no trace arg)
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
        // GPU kernels
        // ─────────────────────────────────────────────────────────────────────

        /// Add two i64 values via a WGSL compute shader.
        /// Values are cast to i32 inside the shader (WGSL has no native i64 on
        /// most hardware without the optional `shader_int64` feature).
        fn gpu_add(&self, a: i64, b: i64) -> i64 {
            const SHADER: &str = r#"
@group(0) @binding(0) var<storage, read>       buf_a   : array<i32>;
@group(0) @binding(1) var<storage, read>       buf_b   : array<i32>;
@group(0) @binding(2) var<storage, read_write> buf_out : array<i32>;

@compute @workgroup_size(1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    buf_out[0] = buf_a[0] + buf_b[0];
}
"#;
            let device = &*self.device;
            let queue = &*self.queue;

            // Compile shader
            let shader = device.create_shader_module(ShaderModuleDescriptor {
                label: Some("uadd"),
                source: ShaderSource::Wgsl(SHADER.into()),
            });

            // Input buffers (host → GPU)
            let a_bytes: [i32; 1] = [a as i32];
            let b_bytes: [i32; 1] = [b as i32];

            let buf_a = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("a"),
                contents: bytemuck::cast_slice(&a_bytes),
                usage: BufferUsages::STORAGE,
            });
            let buf_b = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("b"),
                contents: bytemuck::cast_slice(&b_bytes),
                usage: BufferUsages::STORAGE,
            });

            // Output buffer (GPU write) + readback buffer (GPU → host)
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

            // Bind group layout
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

            // Pipeline
            // wgpu 29: bind_group_layouts is &[Option<&BindGroupLayout>], no push_constant_ranges
            let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&bgl)],
                ..Default::default()
            });
            let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("uadd_pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });

            // Encode dispatch + copy
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

            // Map readback and block until GPU finishes
            // wgpu 29: PollType::wait_indefinitely() is a convenience constructor
            let buf_slice = buf_readback.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            buf_slice.map_async(MapMode::Read, move |v| tx.send(v).unwrap());
            device
                .poll(PollType::wait_indefinitely())
                .expect("GPU poll failed");
            rx.recv().unwrap().unwrap();

            let mapped = buf_slice.get_mapped_range();
            let result: i32 = bytemuck::cast_slice::<u8, i32>(&mapped)[0];
            drop(mapped);
            buf_readback.unmap();

            result as i64
        }
    }

    impl HardwareExecutor for WgpuExecutor {
        fn name(&self) -> &'static str { "wgpu" }

        fn exec(&self, block: &IRBlock, ctx: &HashMap<String, PayloadValue>) -> PayloadValue {
            match &block.opcode {
                Opcode::UAdd => {
                    let a = ctx.get(&block.inputs[0]).expect("missing input a");
                    let b = ctx.get(&block.inputs[1]).expect("missing input b");
                    match (a, b) {
                        (PayloadValue::I64(x), PayloadValue::I64(y)) => {
                            PayloadValue::I64(self.gpu_add(*x, *y))
                        }
                        _ => panic!("WgpuExecutor::UAdd requires i64 inputs"),
                    }
                }
                // All other opcodes run on CPU
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
