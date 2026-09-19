# File: Declares deployment identity and scaling inputs with defensive validation.
variable "namespace" {
  description = "Dedicated Kubernetes namespace."
  type        = string
  default     = "job-orchestrator"
  validation {
    condition     = can(regex("^[a-z0-9-]{1,63}$", var.namespace))
    error_message = "Namespace must be a valid lowercase DNS label."
  }
}

variable "worker_replicas" {
  description = "Initial bounded worker replica count."
  type        = number
  default     = 3
  validation {
    condition     = var.worker_replicas >= 1 && var.worker_replicas <= 20
    error_message = "Worker replicas must be between 1 and 20."
  }
}

