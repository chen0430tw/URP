use std::collections::HashMap;

use crate::ir::MergeMode;
use crate::packet::PayloadValue;

pub trait Reducer {
    fn merge(&self, values: &[PayloadValue]) -> String;
}

pub struct ListReducer;
pub struct SumReducer;
pub struct ConcatReducer;
pub struct ReduceMaxReducer;

impl Reducer for ListReducer {
    fn merge(&self, values: &[PayloadValue]) -> String {
        let rendered: Vec<String> = values.iter().map(render_value).collect();
        format!("{:?}", rendered)
    }
}

impl Reducer for SumReducer {
    fn merge(&self, values: &[PayloadValue]) -> String {
        let sum: i64 = values.iter().filter_map(|v| match v {
            PayloadValue::I64(x) => Some(*x),
            _ => None,
        }).sum();
        sum.to_string()
    }
}

impl Reducer for ConcatReducer {
    fn merge(&self, values: &[PayloadValue]) -> String {
        values.iter().map(render_value).collect::<Vec<_>>().join("")
    }
}

impl Reducer for ReduceMaxReducer {
    fn merge(&self, values: &[PayloadValue]) -> String {
        let max = values.iter().filter_map(|v| match v {
            PayloadValue::I64(x) => Some(*x),
            _ => None,
        }).max().unwrap_or_default();
        max.to_string()
    }
}

pub fn run_reducers(grouped: &HashMap<MergeMode, Vec<PayloadValue>>) -> HashMap<String, String> {
    let mut out = HashMap::new();

    for (mode, vals) in grouped {
        let reducer: Box<dyn Reducer> = match mode {
            MergeMode::List => Box::new(ListReducer),
            MergeMode::Sum => Box::new(SumReducer),
            MergeMode::Concat => Box::new(ConcatReducer),
            MergeMode::ReduceMax => Box::new(ReduceMaxReducer),
        };
        out.insert(format!("{:?}", mode), reducer.merge(vals));
    }

    out
}

fn render_value(v: &PayloadValue) -> String {
    match v {
        PayloadValue::I64(x) => x.to_string(),
        PayloadValue::Str(s) => s.clone(),
    }
}
