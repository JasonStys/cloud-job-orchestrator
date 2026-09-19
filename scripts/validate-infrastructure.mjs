/** File: Performs dependency-free deployment policy checks before deeper Terraform and container gates. Functions: requireText reports missing safeguards. Variables: files contains the reviewed infrastructure surface. */
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const repository = resolve(import.meta.dirname, "..");
const files = {
  dockerfile: await readFile(resolve(repository, "Dockerfile"), "utf8"),
  workloads: await readFile(resolve(repository, "infra/k8s/workloads.yaml"), "utf8"),
  resilience: await readFile(resolve(repository, "infra/k8s/resilience.yaml"), "utf8"),
  terraform: await readFile(resolve(repository, "infra/terraform/versions.tf"), "utf8")
};
const problems = [];
function requireText(source, pattern, message) { if (!pattern.test(source)) problems.push(message); }

requireText(files.dockerfile, /USER 10001:10001/, "runtime image must be non-root");
requireText(files.workloads, /readOnlyRootFilesystem:\s*true/g, "Kubernetes roots must be read-only");
requireText(files.workloads, /allowPrivilegeEscalation:\s*false/g, "privilege escalation must be disabled");
requireText(files.workloads, /resources:[\s\S]*limits:/, "resource limits are required");
requireText(files.workloads, /readinessProbe:/, "readiness probe is required");
requireText(files.resilience, /kind:\s*NetworkPolicy/, "default-deny network policy is required");
requireText(files.resilience, /kind:\s*PodDisruptionBudget/, "disruption budget is required");
requireText(files.terraform, /required_version\s*=\s*">=/, "Terraform version must be constrained");
if (/:latest\b/.test(Object.values(files).join("\n"))) problems.push("mutable latest image tags are forbidden");

if (problems.length) { console.error(problems.join("\n")); process.exitCode = 1; }
else console.log("infrastructure policy validation passed");

