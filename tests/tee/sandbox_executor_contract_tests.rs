use std::fs;
use std::path::PathBuf;

fn executor_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/tee/sandbox/scripts/sandbox_executor.cjs");
    fs::read_to_string(path).expect("sandbox executor source should be readable")
}

#[test]
fn fill_dispatches_react_compatible_input_events() {
    let source = executor_source();

    assert!(
        source.contains("HTMLInputElement.prototype"),
        "fill must use the native input value setter for controlled inputs"
    );
    assert!(
        source.contains("HTMLTextAreaElement.prototype"),
        "fill must support textarea value setters"
    );
    assert!(
        source.contains("new InputEvent"),
        "fill must dispatch InputEvent so React-style listeners observe the change"
    );
    assert!(
        source.contains("inputType: 'insertText'"),
        "fill must describe text insertion in emitted input events"
    );
    assert!(
        source.contains("new Event('change', { bubbles: true })"),
        "fill must also emit a bubbling change event"
    );
}

#[test]
fn execute_script_helper_does_not_expose_secret_sink_methods() {
    let source = executor_source();

    assert!(
        !source.contains("async fill(selector, field)"),
        "execute_script helper must not expose credbridge.fill"
    );
    assert!(
        !source.contains("async setCookie(valueField, nameField)"),
        "execute_script helper must not expose credential-backed cookie injection"
    );
}

#[test]
fn lightpanda_cdp_server_uses_explicit_idle_timeout() {
    let source = executor_source();

    assert!(
        source.contains("LIGHTPANDA_CDP_IDLE_TIMEOUT_SECS"),
        "executor must expose a configurable CDP idle timeout"
    );
    assert!(
        source.contains("DEFAULT_LIGHTPANDA_CDP_IDLE_TIMEOUT_SECS = 60"),
        "executor should default above Lightpanda's 10 second idle timeout"
    );
    assert!(
        source.contains("'--timeout'"),
        "lightpanda serve must receive an explicit timeout"
    );
    assert!(
        source.contains("resolveLightpandaCdpIdleTimeoutSeconds()"),
        "lightpanda serve timeout should be resolved before spawn"
    );
}
