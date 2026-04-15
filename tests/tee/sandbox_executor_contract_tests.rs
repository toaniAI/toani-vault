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
fn credential_script_fill_uses_same_input_semantics() {
    let source = executor_source();

    assert!(
        source.contains("setInputValueInPage(element, value);"),
        "credbridge.fill helper must share the controlled-input fill path"
    );
    assert!(
        !source.contains("element.value = value;\n                element.dispatchEvent"),
        "credbridge.fill must not directly assign .value with only generic events"
    );
}
