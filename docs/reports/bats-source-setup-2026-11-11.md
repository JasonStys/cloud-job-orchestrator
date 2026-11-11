# Recovery-test runner setup correction

By Jason Stys. The current-guide pull request's repository job 130615649181 failed while APT could not resolve the Ubuntu package mirrors. No recovery assertion had started. The application, Rust workers and published assets were not involved in that failure.

The unchanged recovery cases now use official [Bats 1.10.0 source](https://github.com/bats-core/bats-core/tree/f7defb94362f2053a3e73d13086a167448ea9133), with its annotated upstream tag independently resolved to that exact commit. A credential-free secondary checkout stays in the excluded job-local `.runtime/bats-core` tree. The real executable must report exactly `Bats 1.10.0` before it runs the same `scripts/tests/recovery.bats`. No global package install or ignored installer failure is used.

Real workflow-mutation fixtures reject a moving ref, different source, omitted version check, missing original cases and changed deadline. Original ShellCheck, source/privacy/infrastructure/index checks and the ten-minute job bound remain. Local Windows does not prove the Linux Bash cases; fresh actual hosted execution is required. This correction changes CI/tooling only, preserves the current illustrated guide bytes and does not certify future package availability.
