# Engineering Research Notes

Primary references used to shape the implementation:

- [PostgreSQL `SELECT` locking clause](https://www.postgresql.org/docs/18/sql-select.html#SQL-FOR-UPDATE-SHARE) for row locks and `SKIP LOCKED` queue semantics.
- [PostgreSQL transaction isolation](https://www.postgresql.org/docs/18/transaction-iso.html) for understanding concurrent statement visibility.
- [PostgreSQL 18 documentation](https://www.postgresql.org/docs/18/) for the current supported major release used by CI.
- [Rust API guidelines](https://rust-lang.github.io/api-guidelines/) for type-driven interfaces, predictable naming, and documentation.
- [The Rust Clippy book](https://doc.rust-lang.org/clippy/) for warning-free static analysis.
- [Axum documentation](https://docs.rs/axum/latest/axum/) for typed extraction and router composition.
- [SQLx documentation](https://docs.rs/sqlx/latest/sqlx/) for asynchronous pooling, transactions, and runtime-bound queries.
- [Kubernetes Pod Security Standards](https://kubernetes.io/docs/concepts/security/pod-security-standards/) for restricted workload settings.
- [Kubernetes probes](https://kubernetes.io/docs/concepts/configuration/liveness-readiness-startup-probes/) for liveness/readiness separation.
- [Terraform style guide](https://developer.hashicorp.com/terraform/language/style) for constrained, reviewable configuration.
- [GitHub CodeQL compiled-language guidance](https://docs.github.com/en/code-security/concepts/code-scanning/codeql/codeql-for-compiled-languages) for Rust `none` build mode.
- [GitHub PostgreSQL service containers](https://docs.github.com/en/actions/tutorials/use-containerized-services/create-postgresql-service-containers) for real-database CI isolation.

## Language choices

Rust makes the state machine and worker operation allowlist explicit while providing async network/database performance without a garbage collector. SQL keeps concurrency authority near durable rows and uses transactional compare-and-set updates. TypeScript gives the dashboard strict API/view contracts while shipping plain browser JavaScript. Bash remains appropriate for composable database backup/restore operations. Terraform and Kubernetes YAML express reviewable infrastructure and runtime policy.

