//! ステートマシンと実行をメモリに持つ。再起動で消える。

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::asl::Definition;

#[derive(Debug, Clone)]
pub struct StateMachine {
    pub arn: String,
    pub name: String,
    /// 受け取った定義の文字列（DescribeStateMachine でそのまま返す）。
    pub definition_text: String,
    pub definition: Arc<Definition>,
    pub role_arn: String,
    /// `STANDARD` か `EXPRESS`。今はどちらも同じように動かす。
    pub kind: String,
    pub creation_ms: i64,
}

#[derive(Debug, Clone)]
pub struct Execution {
    pub arn: String,
    pub state_machine_arn: String,
    pub name: String,
    pub input: String,
    pub start_ms: i64,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Running,
    Succeeded {
        stop_ms: i64,
        output: String,
    },
    Failed {
        stop_ms: i64,
        error: Option<String>,
        cause: Option<String>,
    },
    TimedOut {
        stop_ms: i64,
    },
}

/// CreateStateMachine の結果。
pub enum Created {
    New(StateMachine),
    /// 同じ名前・定義・ロールがすでにある（冪等に成功する）。
    Existing(StateMachine),
    /// 同じ名前で中身が違う。
    Conflict,
}

/// StartExecution の結果。
pub enum Started {
    New(Execution),
    /// 同じ名前の実行が RUNNING で、入力もバイト単位で同じ（冪等に成功する。2026-09-07 実測）。
    Existing(Execution),
    /// 同じ名前の実行がある（終わっているか、入力が違う）。
    Conflict,
}

#[derive(Default)]
pub struct Store {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// キーは ARN。一覧が名前順になるよう BTreeMap にする（本物の順序は未実測）。
    state_machines: BTreeMap<String, StateMachine>,
    executions: HashMap<String, Execution>,
}

impl Store {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // 保持中に panic するコードは無いので、poison しても中身は壊れていない。
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn create_state_machine(&self, state_machine: StateMachine) -> Created {
        let mut inner = self.lock();
        match inner.state_machines.get(&state_machine.arn) {
            Some(existing)
                if existing.definition_text == state_machine.definition_text
                    && existing.role_arn == state_machine.role_arn =>
            {
                Created::Existing(existing.clone())
            }
            Some(_) => Created::Conflict,
            None => {
                inner
                    .state_machines
                    .insert(state_machine.arn.clone(), state_machine.clone());
                Created::New(state_machine)
            }
        }
    }

    pub fn state_machine(&self, arn: &str) -> Option<StateMachine> {
        self.lock().state_machines.get(arn).cloned()
    }

    pub fn state_machines(&self) -> Vec<StateMachine> {
        self.lock().state_machines.values().cloned().collect()
    }

    pub fn start_execution(&self, execution: Execution) -> Started {
        let mut inner = self.lock();
        match inner.executions.get(&execution.arn) {
            Some(existing)
                if existing.status == Status::Running
                    && existing.input.as_bytes() == execution.input.as_bytes() =>
            {
                Started::Existing(existing.clone())
            }
            Some(_) => Started::Conflict,
            None => {
                inner
                    .executions
                    .insert(execution.arn.clone(), execution.clone());
                Started::New(execution)
            }
        }
    }

    /// 実行を終端状態にする。すでに終わっていれば何もしない。
    pub fn finish(&self, arn: &str, status: Status) {
        if let Some(execution) = self.lock().executions.get_mut(arn)
            && execution.status == Status::Running
        {
            execution.status = status;
        }
    }

    pub fn execution(&self, arn: &str) -> Option<Execution> {
        self.lock().executions.get(arn).cloned()
    }
}
