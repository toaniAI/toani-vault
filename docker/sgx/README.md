# SGX Build Image

This image is for Linux SGX builders and Drone SGX runners.

It is not intended to be used as the normal application runtime image. Its job is to:

- build the Phase A enclave skeleton
- sign the enclave artifact
- run `tee-hardware` compile checks
- provide a consistent TEE-oriented build container for later hardware tests
