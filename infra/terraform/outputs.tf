output "api_elastic_ip" {
  description = "Vercel DNS A record target for api.codexpt.jappi.net.pe."
  value       = aws_eip.api.public_ip
}

output "api_domain" {
  value = var.api_domain
}

output "web_origin" {
  value = var.web_origin
}

output "cognito_issuer" {
  description = "OIDC issuer configured for the API."
  value       = var.identity_provider == "cognito" ? "https://${aws_cognito_user_pool.codexpt[0].endpoint}" : null
}

output "cognito_user_pool_id" {
  value = var.identity_provider == "cognito" ? aws_cognito_user_pool.codexpt[0].id : null
}

output "cognito_client_id" {
  description = "Public OIDC client identifier; it is not a secret."
  value       = var.identity_provider == "cognito" ? aws_cognito_user_pool_client.codexpt[0].id : null
}

output "cognito_managed_login_url" {
  value = var.identity_provider == "cognito" ? "https://${aws_cognito_user_pool_domain.codexpt[0].domain}.auth.${var.aws_region}.amazoncognito.com" : null
}

output "oidc_issuer" {
  description = "Selected identity issuer used by the backend."
  value       = local.oidc_issuer
}

output "oidc_client_id" {
  description = "Selected public OIDC client ID. Not a secret."
  value       = local.oidc_client_id
}

output "backup_bucket" {
  value = aws_s3_bucket.backup.bucket
}

