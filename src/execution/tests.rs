use super::*;
use crate::execution::runtime::{OutputSchema, Request, model_input, run_model};
use crate::work::store::{BeginWork, Store, StoreOptions};
use std::{cell::Cell, collections::BTreeSet, rc::Rc};

struct Synthetic {
    profile: Profile,
    effective: Profile,
    calls: Rc<Cell<usize>>,
}
impl Integration for Synthetic {
    fn profile(&self) -> &Profile {
        &self.profile
    }
    fn invoke(
        &self,
        request: Invocation,
        _observer: &mut dyn FnMut(u32) -> Result<()>,
    ) -> Result<Receipt> {
        assert_eq!(request.deadline_ms, 1000);
        assert!(!request.prompt.is_empty());
        assert!(request.schema.is_object());
        self.calls.set(self.calls.get() + 1);
        Ok(Receipt {
            value: json!(
                json!({"answer":"Synthetic answer.","evidence":[],"uncertainties":[]}).to_string()
            ),
            effective_profile: Some(self.effective.clone()),
            report: json!({"outcome":"completed","cleanup":"confirmed","turnAccepted":"confirmed","usage":if self.profile.model=="unmeasured" {Value::Null}else{json!({"totalTokens":10,"inputTokens":7,"outputTokens":3,"cachedInputTokens":0,"reasoningOutputTokens":0})}}),
        })
    }
}
fn options(identity: &Value) -> BeginWork {
    BeginWork {
        key: hash(&identity.to_string()),
        kind: "ask".into(),
        max_calls: Some(1),
        max_input_bytes: Some(131072),
        remaining: vec![],
        result_key: Some("result".into()),
        snapshot: "fixture".into(),
        warning_baseline: None,
    }
}
fn identity(execution: &Execution, task: &str) -> Value {
    json!({"task":task,"sources":[],"snapshot":"fixture","model":execution.cache_identity(),"automatic":1})
}

#[test]
fn integration_and_profile_changes_isolate_results_and_preserve_retained_work() {
    let directory = std::env::temp_dir().join(format!("hivex-profile-{}", uuid::Uuid::new_v4()));
    let mut store = Store::open(&directory, StoreOptions::default()).unwrap();
    store.save_graph(&Store::empty_graph()).unwrap();
    let calls = Rc::new(Cell::new(0));
    let request = Request {
        instruction: "Answer using supplied data.".into(),
        packet: json!({"task":"fixture"}),
        schema: OutputSchema::Answer,
        stage: "ask".into(),
    };
    let mut fingerprints = BTreeSet::new();
    let mut work_ids = BTreeSet::new();
    for (integration, provider, model, option, value) in [
        ("synthetic", "vendor-a", "model-a", "effort", "deep"),
        ("synthetic", "vendor-a", "model-b", "effort", "deep"),
        ("synthetic", "vendor-a", "model-b", "temperature", "0.2"),
        ("other-agent", "vendor-b", "model-c", "thinking", "enabled"),
        (
            "other-agent",
            "vendor-b",
            "unmeasured",
            "thinking",
            "enabled",
        ),
    ] {
        let profile = Profile {
            integration: integration.into(),
            provider: provider.into(),
            model: model.into(),
            options: BTreeMap::from([(option.into(), value.into())]),
        };
        let execution = Execution::new(
            Box::new(Synthetic {
                profile: profile.clone(),
                effective: profile,
                calls: calls.clone(),
            }),
            1000,
        );
        assert!(fingerprints.insert(model_input(&request, &execution).fingerprint));
        let id = identity(&execution, "fixture");
        let mut work = store
            .begin_with_profile(options(&id), execution.binding(&id))
            .unwrap();
        assert_eq!(work.calls(), 0);
        assert!(work_ids.insert(work.id().to_owned()));
        let before = calls.get();
        let result = run_model(&mut work, &mut store, &execution, &request)
            .unwrap()
            .unwrap();
        assert_eq!(calls.get(), before + 1);
        assert_eq!(work.calls(), 1);
        assert_eq!(
            work.total_tokens(),
            if model == "unmeasured" { 0 } else { 10 }
        );
        assert_eq!(
            runtime::work_summary(&work)["unmeasuredAttempts"],
            if model == "unmeasured" { 1 } else { 0 }
        );
        work.finish_round();
        work.complete(result.clone(), "result".into()).unwrap();
        store.save(&mut work).unwrap();
        let mut reused = store
            .begin_with_profile(options(&id), execution.binding(&id))
            .unwrap();
        assert_eq!(
            run_model(&mut reused, &mut store, &execution, &request).unwrap(),
            Some(result)
        );
        assert_eq!(calls.get(), before + 1);
        assert_eq!(reused.id(), work.id());
    }
    assert_eq!(store.works().unwrap().len(), 5);
    assert_eq!(store.graph().unwrap(), Store::empty_graph());
    let a = Profile {
        integration: "synthetic".into(),
        provider: "vendor-a".into(),
        model: "model-a".into(),
        options: BTreeMap::new(),
    };
    let mut b = a.clone();
    b.model = "model-b".into();
    let execution_a = Execution::new(
        Box::new(Synthetic {
            profile: a.clone(),
            effective: a.clone(),
            calls: calls.clone(),
        }),
        1000,
    );
    let execution_b = Execution::new(
        Box::new(Synthetic {
            profile: b.clone(),
            effective: b,
            calls: calls.clone(),
        }),
        1000,
    );
    let id_a = identity(&execution_a, "pending");
    let mut work = store
        .begin_with_profile(options(&id_a), execution_a.binding(&id_a))
        .unwrap();
    run_model(&mut work, &mut store, &execution_a, &request).unwrap();
    let retained = work.value().clone();
    let count = calls.get();
    let id_b = identity(&execution_b, "pending");
    assert_eq!(
        store
            .begin_with_profile(options(&id_b), execution_b.binding(&id_b))
            .err()
            .unwrap()
            .code,
        "EXECUTION_PROFILE_CHANGED"
    );
    assert_eq!(
        run_model(&mut work, &mut store, &execution_b, &request)
            .err()
            .unwrap()
            .code,
        "EXECUTION_PROFILE_CHANGED"
    );
    assert_eq!(calls.get(), count);
    assert_eq!(work.value(), &retained);
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn an_effective_profile_mismatch_is_retained_as_a_failed_paid_attempt() {
    let directory =
        std::env::temp_dir().join(format!("hivex-profile-mismatch-{}", uuid::Uuid::new_v4()));
    let mut store = Store::open(&directory, StoreOptions::default()).unwrap();
    store.save_graph(&Store::empty_graph()).unwrap();
    let profile = Profile {
        integration: "synthetic".into(),
        provider: "vendor-a".into(),
        model: "requested".into(),
        options: BTreeMap::new(),
    };
    let mut effective = profile.clone();
    effective.model = "unrequested".into();
    let calls = Rc::new(Cell::new(0));
    let execution = Execution::new(
        Box::new(Synthetic {
            profile,
            effective,
            calls: calls.clone(),
        }),
        1000,
    );
    let id = identity(&execution, "mismatch");
    let mut work = store
        .begin_with_profile(options(&id), execution.binding(&id))
        .unwrap();
    let request = Request {
        instruction: "Answer.".into(),
        packet: json!({}),
        schema: OutputSchema::Answer,
        stage: "ask".into(),
    };
    assert!(
        run_model(&mut work, &mut store, &execution, &request)
            .unwrap()
            .is_none()
    );
    assert_eq!(work.status(), crate::work::State::Failed);
    assert_eq!(work.calls(), 1);
    assert_eq!(work.total_tokens(), 10);
    assert_eq!(
        work.value()["attempts"][0]["report"]["code"],
        "EXECUTION_PROFILE_MISMATCH"
    );
    assert_eq!(
        work.value()["attempts"][0]["report"]["executionProfile"]["model"],
        "unrequested"
    );
    assert_eq!(store.graph().unwrap(), Store::empty_graph());
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}
