pub(crate) mod runtime;
#[cfg(test)]
mod tests;

use crate::documents::markdown::hash;
use crate::error::Result;
use crate::work::store::ExecutionBinding;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub integration: String,
    pub provider: String,
    pub model: String,
    pub options: BTreeMap<String, String>,
}

pub struct Invocation {
    pub prompt: String,
    pub schema: Value,
    pub deadline_ms: u64,
}

/// Integration evidence is retained; unknown usage is explicit and profile mismatches become failures.
pub struct Receipt {
    pub value: Value,
    pub report: Value,
    pub effective_profile: Option<Profile>,
}
impl Receipt {
    pub fn completed(&self) -> bool {
        self.report["outcome"] == "completed" && self.report["cleanup"] == "confirmed"
    }
}

/// The only replaceable integration seam. It owns protocol, profile admission and lifecycle.
pub trait Integration {
    fn profile(&self) -> &Profile;
    fn model_summary(&self) -> Value {
        json!(self.profile())
    }
    fn cache_identity(&self) -> Value {
        json!(self.profile())
    }
    fn legacy_cache_identity(&self) -> Option<Value> {
        None
    }
    fn invoke(
        &self,
        request: Invocation,
        on_process: &mut dyn FnMut(u32) -> Result<()>,
    ) -> Result<Receipt>;
}

pub struct Execution {
    integration: Box<dyn Integration>,
    profile: Profile,
    identity: Value,
    legacy_identity: Option<Value>,
    summary: Value,
    pub deadline_ms: u64,
}
impl Execution {
    pub fn new(integration: Box<dyn Integration>, deadline_ms: u64) -> Self {
        let profile = integration.profile().clone();
        let identity = integration.cache_identity();
        let legacy_identity = integration.legacy_cache_identity();
        let summary = integration.model_summary();
        Self {
            integration,
            profile,
            identity,
            legacy_identity,
            summary,
            deadline_ms,
        }
    }
    pub fn profile(&self) -> &Profile {
        &self.profile
    }
    pub fn cache_identity(&self) -> Value {
        self.identity.clone()
    }
    pub fn model_summary(&self) -> Value {
        self.summary.clone()
    }
    pub fn binding(&self, identity: &Value) -> ExecutionBinding {
        let mut continuity = identity.clone();
        continuity
            .as_object_mut()
            .expect("work identity is an object")
            .shift_remove("model");
        let legacy_key = self.legacy_identity.clone().map(|model| {
            let mut legacy = identity.clone();
            legacy["model"] = model;
            hash(&legacy.to_string())
        });
        ExecutionBinding {
            operation_key: hash(&continuity.to_string()),
            profile: json!(self.profile()),
            legacy_key,
        }
    }
    pub fn invoke(
        &self,
        prompt: String,
        schema: Value,
        on_process: &mut dyn FnMut(u32) -> Result<()>,
    ) -> Result<Receipt> {
        let mut receipt = self.integration.invoke(
            Invocation {
                prompt,
                schema,
                deadline_ms: self.deadline_ms,
            },
            on_process,
        )?;
        if receipt.completed() && receipt.effective_profile.as_ref() != Some(self.profile()) {
            receipt.value = Value::Null;
            receipt.report["outcome"] = json!("failed");
            receipt.report["code"] = json!("EXECUTION_PROFILE_MISMATCH");
        }
        if let Some(profile) = &receipt.effective_profile {
            receipt.report["executionProfile"] = json!(profile);
        }
        Ok(receipt)
    }
}
