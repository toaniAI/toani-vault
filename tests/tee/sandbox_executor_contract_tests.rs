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
fn bootstrap_page_contract_only_reinjects_external_scripts() {
    let source = executor_source();

    assert!(
        source.contains("DEFAULT_BOOTSTRAP_SCRIPT_SELECTORS"),
        "bootstrap_page should keep a controlled default selector set"
    );
    assert!(
        source.contains("DEFAULT_BOOTSTRAP_DISCOVERY_TIMEOUT_MS = 5000"),
        "bootstrap_page should cap pre-scan discovery waits so bootstrap does not hang indefinitely"
    );
    assert!(
        source.contains("script[src]"),
        "bootstrap_page should broadly inspect external scripts before type filtering"
    );
    assert!(
        source.contains("script[src][type$=\"-text/javascript\"]"),
        "bootstrap_page should still target Rocket Loader style external scripts"
    );
    assert!(
        source.contains("script[src][type=\"text/javascript\"]"),
        "bootstrap_page should also discover standard external scripts"
    );
    assert!(
        source.contains("script[src][type=\"application/javascript\"]"),
        "bootstrap_page should also discover application/javascript external scripts"
    );
    assert!(
        source.contains("script[src][type=\"module\"]"),
        "bootstrap_page should also discover module scripts"
    );
    assert!(
        source.contains("script[src][defer]"),
        "bootstrap_page should discover deferred external scripts"
    );
    assert!(
        source.contains("script[src][nomodule]"),
        "bootstrap_page should discover nomodule scripts"
    );
    assert!(
        source.contains("script[src]:not([type])"),
        "bootstrap_page should discover external scripts without a type attribute"
    );
    assert!(
        source.contains("node.getAttribute('src')"),
        "bootstrap_page should read the raw src attribute when Lightpanda does not populate node.src"
    );
    assert!(
        source.contains("new URL(normalizedAttributeSrc, document.baseURI).href"),
        "bootstrap_page should resolve relative script src attributes against document.baseURI"
    );
    assert!(
        source.contains("waitForFunction(() => !!document.body"),
        "bootstrap_page should wait for document.body before scanning"
    );
    assert!(
        source.contains("document.readyState !== 'loading'"),
        "bootstrap_page should not immediately scan a partially parsed document"
    );
    assert!(
        source.contains("DEFAULT_BOOTSTRAP_RESCAN_DELAY_MS = 250"),
        "bootstrap_page should do a controlled delayed rescan when the first scan finds nothing"
    );
    assert!(
        source.contains("bootstrap_page mode must be rocket_loader"),
        "bootstrap_page should hard reject unsupported modes"
    );
    assert!(
        source.contains("document.createElement('script')"),
        "bootstrap_page must recreate external script tags instead of executing raw JS"
    );
    assert!(
        source.contains("injected.defer = currentDescriptor.defer === true"),
        "bootstrap_page should preserve defer during reinjection"
    );
    assert!(
        source.contains("injected.noModule = currentDescriptor.noModule === true"),
        "bootstrap_page should preserve nomodule during reinjection"
    );
    assert!(
        source.contains("injected.crossOrigin = currentDescriptor.crossOrigin"),
        "bootstrap_page should preserve crossorigin during reinjection"
    );
    assert!(
        source.contains("injected.referrerPolicy = currentDescriptor.referrerPolicy"),
        "bootstrap_page should preserve referrerpolicy during reinjection"
    );
    assert!(
        source.contains("data-credbridge-bootstrap-status"),
        "bootstrap_page should track reinjected script load state for diagnostics"
    );
    assert!(
        source.contains("DEFAULT_BOOTSTRAP_POST_INJECTION_SETTLE_MS = 1500"),
        "bootstrap_page should give reinjected bundles a short settle window before checking page mount results"
    );
    assert!(
        source.contains("ensureAnalyticsCompatibilityShim"),
        "bootstrap_page should install analytics compatibility shims through a dedicated helper"
    );
    assert!(
        source.contains("typeof window.gtag !== 'function'"),
        "bootstrap_page should only shim gtag when the page does not already provide it"
    );
    assert!(
        source.contains("window.dataLayer = []"),
        "bootstrap_page should create a minimal dataLayer when analytics globals are absent"
    );
    assert!(
        source.contains("window.gtag = function gtag()"),
        "bootstrap_page should provide a minimal gtag shim for analytics-safe compatibility"
    );
    assert!(
        source.contains("analytics_compatibility_shim"),
        "bootstrap_page diagnostics should report whether the analytics compatibility shim activated"
    );
    assert!(
        source.contains("compatibility_injections"),
        "bootstrap_page diagnostics should list which fixed compatibility shims were applied"
    );
    assert!(
        source.contains("gtag_before_type"),
        "bootstrap_page diagnostics should expose the pre-injection gtag type"
    );
    assert!(
        source.contains("gtag_after_type"),
        "bootstrap_page diagnostics should expose the post-injection gtag type"
    );
    assert!(
        source.contains("data_layer_initialized"),
        "bootstrap_page diagnostics should report whether dataLayer was initialized"
    );
    assert!(
        source.contains("compatibility_applied"),
        "bootstrap_page diagnostics should report whether any compatibility shim was applied"
    );
    assert!(
        source.contains("injected_script_statuses"),
        "bootstrap_page diagnostics should expose reinjected script statuses when mount still fails"
    );
    assert!(
        source.contains("bootstrap_failed: selector_not_found:"),
        "bootstrap_page must surface selector wait failures with a clear bootstrap_failed error"
    );
    assert!(
        source.contains("ready_state_before_scan"),
        "bootstrap_page diagnostics should include the ready state seen before script discovery"
    );
    assert!(
        source.contains("ready_state_after_injection"),
        "bootstrap_page diagnostics should include the ready state after reinjection"
    );
    assert!(
        source.contains("selectors: scriptSelectors"),
        "bootstrap_page diagnostics should preserve the requested selector list for compatibility"
    );
    assert!(
        source.contains("matched_selectors"),
        "bootstrap_page diagnostics should report which selectors matched"
    );
    assert!(
        source.contains("sample_script_descriptors"),
        "bootstrap_page diagnostics should expose truncated script descriptor samples"
    );
    assert!(
        source.contains("selector_exists_at_failure"),
        "bootstrap_page failures should report selector existence at the time of timeout"
    );
    assert!(
        source.contains("DOMContentLoaded"),
        "bootstrap_page should be able to replay lifecycle events for late-mounted apps"
    );
    assert!(
        source.contains("window.dispatchEvent(new Event('load'))"),
        "bootstrap_page should support replaying the load event when requested"
    );
    assert!(
        !source.contains("injected.text"),
        "bootstrap_page must not replay inline script text"
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
