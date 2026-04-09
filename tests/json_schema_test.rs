use urx_runtime_v08::ir::{IRGraph, IRBlock, IREdge, Opcode, MergeMode};

#[test]
fn test_json_round_trip() {
    // Build a tiny graph
    let mut g = IRGraph::with_id("test_rt".into());
    g.blocks.push(IRBlock {
        block_id: "a".into(),
        opcode: Opcode::FConst(2.5),
        inputs: vec![],
        output: "v".into(),
        required_tag: "cpu".into(),
        merge_mode: MergeMode::List,
        resource_shape: "scalar".into(),
        preferred_zone: "z1".into(),
        inertia_key: None,
        estimated_duration: 1,
    });
    let mut mul = IRBlock::new("mul", Opcode::FMul);
    mul.inputs = vec!["lhs".into(), "rhs".into()];
    g.blocks.push(mul);
    g.edges.push(IREdge {
        src_block: "a".into(), dst_block: "mul".into(),
        output_key: "v".into(), input_key: "lhs".into(),
    });

    // Serialize
    let json = g.to_json().unwrap();
    println!("JSON:\n{json}");

    // Deserialize
    let g2 = IRGraph::from_json(&json).unwrap();
    assert_eq!(g2.graph_id, "test_rt");
    assert_eq!(g2.blocks.len(), 2);
    assert_eq!(g2.edges.len(), 1);
    assert!(matches!(g2.blocks[0].opcode, Opcode::FConst(v) if (v - 2.5).abs() < 1e-9));
    assert!(matches!(g2.blocks[1].opcode, Opcode::FMul));
}

#[test]
fn test_json_schema_opcodes() {
    // Test every opcode category serializes/deserializes correctly
    let opcodes: Vec<Opcode> = vec![
        Opcode::UConstI64(42),
        Opcode::FConst(3.14),
        Opcode::UAdd, Opcode::FAdd, Opcode::FMul, Opcode::FSqrt,
        Opcode::FNeg, Opcode::FDiv, Opcode::FPow,
        Opcode::I64ToF64, Opcode::F64ToI64,
    ];
    for op in opcodes {
        let b = IRBlock::new("b", op.clone());
        let mut g = IRGraph::with_id("op_test".into());
        g.blocks.push(b);
        let json = g.to_json().unwrap();
        let g2 = IRGraph::from_json(&json).unwrap();
        // Just verify it round-trips without panic
        assert_eq!(g2.blocks.len(), 1);
    }
}
