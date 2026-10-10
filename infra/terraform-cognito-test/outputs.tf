output "oidc_issuer" {
  description = "Issuer URL to configure as CODEXPT_OIDC_ISSUER."
  value       = "https://${aws_cognito_user_pool.codexpt_oidc_test.endpoint}"
}

output "oidc_client_id" {
  description = "Public client ID to configure as CODEXPT_OIDC_CLIENT_ID; it is not a secret."
  value       = aws_cognito_user_pool_client.codexpt_oidc_test.id
}

output "managed_login_url" {
  description = "Cognito hosted UI base URL for the real loopback OIDC test."
  value       = "https://${aws_cognito_user_pool_domain.codexpt_oidc_test.domain}.auth.${var.aws_region}.amazoncognito.com"
}

output "callback_url" {
  description = "Exact local callback registered in the public app client."
  value       = "http://127.0.0.1:4173/api/auth/callback"
}

output "logout_url" {
  description = "Exact local post-logout URL registered in the public app client."
  value       = "http://127.0.0.1:4173/"
}

output "user_pool_id" {
  description = "Dedicated pool ID for test-user administration."
  value       = aws_cognito_user_pool.codexpt_oidc_test.id
}
