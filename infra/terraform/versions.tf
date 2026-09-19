# File: Pins Terraform and provider compatibility for reproducible planning.
terraform {
  required_version = ">= 1.8.0, < 2.0.0"
  required_providers {
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.38.0"
    }
  }
}

