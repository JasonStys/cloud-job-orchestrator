# Locked advisory patch and read-only audit gate

The first accepted native feature PR passed its original checks. During the later
exact-main run, a fresh RustSec database reported `RUSTSEC-2026-0486` for the locked
`rustix` 1.1.5 extended-attribute buffer behavior. The advisory identifies 1.1.6 as
patched. `Cargo.lock` updates only that transitive package and its registry checksum;
no advisory ID or target is ignored.

The old action then failed updating its separate GitHub Check with `Not Found`.
That job already had Check/Issue write permissions. The log does not prove a token
scope cause or an application/native failure. The repair removes this unnecessary
Check/Issue API integration and both write scopes, rather than increasing privileges.

The same named audit job now installs exact `cargo-audit` 0.23.0 with its locked
graph and runs the official CLI against the complete project lockfile. Its ordinary
current-database fetch stays enabled, and `--deny warnings` makes informational
warnings fail alongside vulnerabilities. Explicit Bash `pipefail` preserves the
actual scanner exit while retaining JSON in logs. A missing database/network/tool
error also fails; it is never treated as a clean result.

Six actual-workflow/mutation checks reject removed warning enforcement, extra
write privileges, advisory suppression, disabled fetching and masked pipeline
failure. They describe the reviewed literal workflow format, not a general YAML
parser or a substitute for running the real advisory scanner. Original tests,
database/native/source checks, coverage floors and release acceptance remain.

This source is a candidate correction. Its fresh hosted audit and exact-main checks
must pass before publication; the earlier native ZIP remains preserved evidence
for its original source and is not retargeted.

Primary sources: [upstream rustix buffer repair](https://github.com/bytecodealliance/rustix/commit/8e6aaa6713d2bb64b1fdd8c472e3a32a10a76d70),
[advisory identity](https://rustsec.org/advisories/RUSTSEC-2026-0486.html),
[official cargo-audit options](https://github.com/rustsec/rustsec/blob/main/cargo-audit/src/commands/audit.rs).
