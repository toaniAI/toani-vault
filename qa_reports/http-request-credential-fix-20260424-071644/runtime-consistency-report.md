# Runtime Consistency Report

## Positive runtime consistency

- Created credential `e2e-http-request-credential-20260424-071644` was successfully bound to sandbox session creation.
- Remote `http_request` execution resolved all three API key aliases consistently:
  - `api_key`
  - `key`
  - `apiKey`
- Wrapper behavior was consistent:
  - `prefix` applied correctly to Authorization.
  - `suffix` appended correctly after the credential value.

## Negative runtime consistency

- Remote runtime rejected unsupported wrapper keys.
- Remote runtime rejected non-string `prefix` and non-string `suffix`.
- Remote runtime continued to reject credential-backed `execute_script` bindings.
- Remote runtime continued to reject credential references in `bootstrap_page`.

## Runtime consistency gap

- Persisted operation detail returned by `GET /api/v1/sandbox/operations/:operation_id` does not include `input_params`.
- The expected session operations listing endpoint returned `404`.
- Result: runtime redaction of persisted input parameters remains unproven in this environment.
