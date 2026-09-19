# File: Provisions the namespace and non-secret runtime configuration consumed by Kubernetes manifests.
resource "kubernetes_namespace_v1" "orchestrator" {
  metadata {
    name = var.namespace
    labels = {
      "pod-security.kubernetes.io/enforce" = "restricted"
    }
  }
}

resource "kubernetes_config_map_v1" "orchestrator" {
  metadata {
    name      = "orchestrator-runtime"
    namespace = kubernetes_namespace_v1.orchestrator.metadata[0].name
  }
  data = {
    RUST_LOG        = "info"
    WORKER_REPLICAS = tostring(var.worker_replicas)
  }
}

output "namespace" {
  description = "Namespace containing the control plane."
  value       = kubernetes_namespace_v1.orchestrator.metadata[0].name
}

