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
  value       = "https://${aws_cognito_user_pool.codexpt.endpoint}"
}

output "cognito_user_pool_id" {
  value = aws_cognito_user_pool.codexpt.id
}

output "cognito_client_id" {
  description = "Public OIDC client identifier; it is not a secret."
  value       = aws_cognito_user_pool_client.codexpt.id
}

output "cognito_managed_login_url" {
  value = "https://${aws_cognito_user_pool_domain.codexpt.domain}.auth.${var.aws_region}.amazoncognito.com"
}

output "backup_bucket" {
  value = aws_s3_bucket.backup.bucket
}

