variable "aws_region" {
  description = "AWS region for the disposable CodeXPT Cognito OIDC test pool."
  type        = string
  default     = "us-east-1"
}

variable "aws_account_id" {
  description = "Expected 12-digit AWS account ID; Terraform refuses to plan against any other account."
  type        = string

  validation {
    condition     = can(regex("^[0-9]{12}$", var.aws_account_id))
    error_message = "aws_account_id must be the exact 12-digit CodeXPT test account ID."
  }
}

variable "user_pool_domain_prefix" {
  description = "Globally unique Cognito managed-login prefix for this isolated test pool."
  type        = string

  validation {
    condition     = can(regex("^[a-z0-9-]{1,40}$", var.user_pool_domain_prefix))
    error_message = "Use a unique Cognito domain prefix with lowercase letters, numbers, and hyphens only."
  }
}
