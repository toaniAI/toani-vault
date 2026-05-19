use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use serde_json::json;
use vault_service::oauth_broker::{
    CallbackAdapterDefinition, CallbackAdapterExecutor, CallbackAdapterInvocationError,
    CallbackCoreGate, CallbackCoreGateFailure, GrantFamily,
};

#[derive(Default)]
struct CountingAdapter {
    calls: Arc<AtomicUsize>,
}

impl CallbackAdapterExecutor for CountingAdapter {
    fn execute(
        &self,
        _payload: &serde_json::Value,
    ) -> Result<serde_json::Value, CallbackAdapterInvocationError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(json!({"normalized": true}))
    }
}

#[test]
fn callback_adapter_is_not_invoked_when_transaction_is_missing() {
    let adapter = CountingAdapter::default();
    let definition =
        CallbackAdapterDefinition::new("lark_openapi", "v1", GrantFamily::AuthorizationCodePkce);

    let result = definition.execute_after_core_gate(
        CallbackCoreGate::Failed(CallbackCoreGateFailure::TransactionMissing),
        &adapter,
        &json!({"code": "abc"}),
    );

    assert!(matches!(
        result,
        Err(CallbackAdapterInvocationError::CoreGateFailed(
            CallbackCoreGateFailure::TransactionMissing
        ))
    ));
    assert_eq!(adapter.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn callback_adapter_is_not_invoked_when_state_mismatches() {
    let adapter = CountingAdapter::default();
    let definition =
        CallbackAdapterDefinition::new("lark_openapi", "v1", GrantFamily::AuthorizationCodePkce);

    let result = definition.execute_after_core_gate(
        CallbackCoreGate::Failed(CallbackCoreGateFailure::StateMismatch),
        &adapter,
        &json!({"state": "bad"}),
    );

    assert!(matches!(
        result,
        Err(CallbackAdapterInvocationError::CoreGateFailed(
            CallbackCoreGateFailure::StateMismatch
        ))
    ));
    assert_eq!(adapter.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn callback_adapter_is_not_invoked_when_replay_is_detected() {
    let adapter = CountingAdapter::default();
    let definition =
        CallbackAdapterDefinition::new("lark_openapi", "v1", GrantFamily::AuthorizationCodePkce);

    let result = definition.execute_after_core_gate(
        CallbackCoreGate::Failed(CallbackCoreGateFailure::ReplayDetected),
        &adapter,
        &json!({"state": "replayed"}),
    );

    assert!(matches!(
        result,
        Err(CallbackAdapterInvocationError::CoreGateFailed(
            CallbackCoreGateFailure::ReplayDetected
        ))
    ));
    assert_eq!(adapter.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn callback_adapter_executes_only_after_core_gate_passes() {
    let adapter = CountingAdapter::default();
    let definition =
        CallbackAdapterDefinition::new("lark_openapi", "v1", GrantFamily::AuthorizationCodePkce);

    let result = definition.execute_after_core_gate(
        CallbackCoreGate::Passed,
        &adapter,
        &json!({"code": "ok"}),
    );

    assert_eq!(result.unwrap()["normalized"], true);
    assert_eq!(adapter.calls.load(Ordering::SeqCst), 1);
}
