variable "aws_region" {
  description = "AWS region for the private beta."
  type        = string
  default     = "us-east-1"
}

variable "web_origin" {
  description = "Exact HTTPS origin of the Vercel frontend."
  type        = string
  default     = "https://codexpt.jappi.net.pe"
}

variable "api_domain" {
  description = "DNS name for the Caddy-proxied Rust API. DNS records are not currently managed by this Terraform configuration."
  type        = string
  default     = "api.codexpt.jappi.net.pe"
}

variable "user_pool_domain_prefix" {
  description = "Globally unique Cognito managed-login prefix; account id is appended."
  type        = string
  default     = "codexpt-jappi"
}

variable "instance_type" {
  description = "Single-instance beta size; t4g.small provides 2 vCPU and 2 GiB RAM."
  type        = string
  default     = "t4g.small"
}

variable "root_volume_gib" {
  description = "Encrypted gp3 root volume; the single SQLite database lives under /var/lib/codexpt."
  type        = number
  default     = 20
}

variable "app_release_url" {
  description = "Immutable HTTPS URL for the reviewed Linux ARM64 CodeXPT binary. Required before planning a deployable instance."
  type        = string

  validation {
    condition     = startswith(var.app_release_url, "https://") && !can(regex("(?i)\\.invalid([/:?#]|$)", var.app_release_url))
    error_message = "app_release_url must use HTTPS and must not use the reserved .invalid domain."
  }
}

variable "app_release_sha256" {
  description = "SHA-256 for app_release_url; the instance refuses to install a mismatched binary."
  type        = string

  validation {
    condition     = can(regex("^[a-fA-F0-9]{64}$", var.app_release_sha256))
    error_message = "app_release_sha256 must be exactly 64 hexadecimal characters."
  }
}

variable "budget_alert_email" {
  description = "Email address for AWS actual and forecast budget alerts."
  type        = string

  validation {
    condition     = can(regex("^[^@ ]+@[^@ ]+\\.[^@ ]+$", var.budget_alert_email)) && !can(regex("(?i)\\.invalid$", var.budget_alert_email))
    error_message = "budget_alert_email must be a real email address, not a reserved .invalid example."
  }
}

variable "monthly_budget_usd" {
  description = "Alert threshold for this AWS account; AWS Budgets alerts do not cap spending."
  type        = number
  default     = 20

  validation {
    condition     = var.monthly_budget_usd >= 10 && var.monthly_budget_usd <= 50
    error_message = "Set a monthly alert between USD 10 and USD 50."
  }
}

variable "caddy_image" {
  description = "Pinned Caddy image tag used for automatic ACME TLS termination."
  type        = string
  default     = "caddy:2.11.6-alpine"
}
