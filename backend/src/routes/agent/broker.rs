//! 前端工具往返 broker：SSE 下发 `tool.request`，前端执行后经 `/agent/tool-callback`
//! 回传结果，通过 `oneshot` 通道把结果交回正在等待的工具 `call()`。

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use serde_json::Value;
use tokio::sync::{oneshot, Mutex};

#[derive(Clone, Default)]
pub struct FrontendToolBroker {
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>,
}

impl FrontendToolBroker {
    pub async fn register(&self, run_id: &str, call_id: &str) -> oneshot::Receiver<Value> {
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .await
            .insert(format!("{}:{}", run_id, call_id), tx);
        rx
    }

    pub async fn resolve(&self, run_id: &str, call_id: &str, output: Value) -> bool {
        let key = format!("{}:{}", run_id, call_id);
        let sender = self.pending.lock().await.remove(&key);
        sender.map(|tx| tx.send(output).is_ok()).unwrap_or(false)
    }

    pub async fn cancel_run(&self, run_id: &str) {
        let prefix = format!("{}:", run_id);
        self.pending
            .lock()
            .await
            .retain(|key, _| !key.starts_with(&prefix));
    }
}

pub fn frontend_tool_broker() -> &'static FrontendToolBroker {
    static BROKER: OnceLock<FrontendToolBroker> = OnceLock::new();
    BROKER.get_or_init(FrontendToolBroker::default)
}

#[derive(Debug, Clone)]
pub struct PendingFrontendToolCall {
    #[allow(dead_code)]
    pub id: String,
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
}
