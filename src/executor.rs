use std::collections::HashMap;

use crate::ir::{IRBlock, Opcode};
use crate::packet::PayloadValue;

pub struct BlockExecutor;

impl BlockExecutor {
    pub fn exec(block: &IRBlock, ctx: &HashMap<String, PayloadValue>) -> PayloadValue {
        match &block.opcode {
            Opcode::UConstI64(v) => PayloadValue::I64(*v),
            Opcode::UConstStr(s) => PayloadValue::Str(s.clone()),
            Opcode::UAdd => {
                let a = ctx.get(&block.inputs[0]).expect("missing input a");
                let b = ctx.get(&block.inputs[1]).expect("missing input b");
                match (a, b) {
                    (PayloadValue::I64(x), PayloadValue::I64(y)) => PayloadValue::I64(x + y),
                    _ => panic!("UAdd expects i64 inputs"),
                }
            }
            Opcode::UConcat => {
                let a = ctx.get(&block.inputs[0]).expect("missing input left");
                let b = ctx.get(&block.inputs[1]).expect("missing input right");
                let left = match a {
                    PayloadValue::I64(v) => v.to_string(),
                    PayloadValue::Str(s) => s.clone(),
                };
                let right = match b {
                    PayloadValue::I64(v) => v.to_string(),
                    PayloadValue::Str(s) => s.clone(),
                };
                PayloadValue::Str(format!("{left}{right}"))
            }
        }
    }
}
