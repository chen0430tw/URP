//! WgpuExecutor integration tests
//!
//! Tests:
//! 1. WgpuExecutor initialises (or reports "no adapter" on headless machines)
//! 2. UAdd runs on GPU and returns correct i64 result
//! 3. Non-GPU opcode (UConstI64) falls back to CPU path
//! 4. Negative numbers and zero are handled correctly

#[cfg(feature = "gpu")]
mod gpu_tests {
    use std::collections::HashMap;
    use urx_runtime_v08::{
        HardwareExecutor, IRBlock, Opcode, WgpuExecutor,
    };
    use urx_runtime_v08::packet::PayloadValue;

    /// Attempt to build a WgpuExecutor; return None if no GPU adapter is available.
    /// This lets the tests run (and be skipped) gracefully in headless / CI environments.
    async fn try_init() -> Option<WgpuExecutor> {
        match WgpuExecutor::new().await {
            Ok(ex) => {
                println!("[gpu] adapter: {}", ex.adapter_info);
                Some(ex)
            }
            Err(e) => {
                println!("[gpu] skipping — no GPU adapter: {e}");
                None
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 1 — adapter discovery
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn test_gpu_executor_init() {
        // Simply verify that the constructor either succeeds or returns a clean error.
        match WgpuExecutor::new().await {
            Ok(ex) => println!("[gpu-init] OK — adapter: {}", ex.adapter_info),
            Err(e) => println!("[gpu-init] no adapter (expected on headless): {e}"),
        }
        // Either outcome is acceptable; the test must not panic.
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 2 — UAdd: 3 + 7 = 10
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn test_gpu_uadd_basic() {
        let Some(ex) = try_init().await else { return };

        let mut block = IRBlock::new("add", Opcode::UAdd);
        block.inputs = vec!["a".to_string(), "b".to_string()];

        let mut ctx = HashMap::new();
        ctx.insert("a".to_string(), PayloadValue::I64(3));
        ctx.insert("b".to_string(), PayloadValue::I64(7));

        let result = ex.exec(&block, &ctx);
        assert_eq!(result, PayloadValue::I64(10), "3 + 7 should equal 10");
        println!("[gpu-uadd] 3 + 7 = {:?} ✓", result);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 3 — UAdd: larger values  (123456 + 654321 = 777777)
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn test_gpu_uadd_larger_values() {
        let Some(ex) = try_init().await else { return };

        let mut block = IRBlock::new("add_big", Opcode::UAdd);
        block.inputs = vec!["x".to_string(), "y".to_string()];

        let mut ctx = HashMap::new();
        ctx.insert("x".to_string(), PayloadValue::I64(123_456));
        ctx.insert("y".to_string(), PayloadValue::I64(654_321));

        let result = ex.exec(&block, &ctx);
        assert_eq!(result, PayloadValue::I64(777_777));
        println!("[gpu-uadd] 123456 + 654321 = {:?} ✓", result);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 4 — UAdd: negative + positive  (-100 + 42 = -58)
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn test_gpu_uadd_negative() {
        let Some(ex) = try_init().await else { return };

        let mut block = IRBlock::new("add_neg", Opcode::UAdd);
        block.inputs = vec!["a".to_string(), "b".to_string()];

        let mut ctx = HashMap::new();
        ctx.insert("a".to_string(), PayloadValue::I64(-100));
        ctx.insert("b".to_string(), PayloadValue::I64(42));

        let result = ex.exec(&block, &ctx);
        assert_eq!(result, PayloadValue::I64(-58));
        println!("[gpu-uadd] -100 + 42 = {:?} ✓", result);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 5 — UAdd: both zeros
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn test_gpu_uadd_zeros() {
        let Some(ex) = try_init().await else { return };

        let mut block = IRBlock::new("add_zero", Opcode::UAdd);
        block.inputs = vec!["a".to_string(), "b".to_string()];

        let mut ctx = HashMap::new();
        ctx.insert("a".to_string(), PayloadValue::I64(0));
        ctx.insert("b".to_string(), PayloadValue::I64(0));

        let result = ex.exec(&block, &ctx);
        assert_eq!(result, PayloadValue::I64(0));
        println!("[gpu-uadd] 0 + 0 = {:?} ✓", result);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Test 6 — Non-GPU opcode falls back to CPU (UConstI64)
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn test_gpu_cpu_fallback() {
        let Some(ex) = try_init().await else { return };

        let block = IRBlock::new("const42", Opcode::UConstI64(42));
        let ctx = HashMap::new();

        let result = ex.exec(&block, &ctx);
        assert_eq!(result, PayloadValue::I64(42));
        assert_eq!(ex.name(), "wgpu");
        println!("[gpu-fallback] UConstI64(42) via CPU path = {:?} ✓", result);
    }
}
