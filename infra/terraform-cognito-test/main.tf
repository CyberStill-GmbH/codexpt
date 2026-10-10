resource "aws_cognito_user_pool" "codexpt_oidc_test" {
  name                     = "codexpt-oidc-loopback-test"
  username_attributes      = ["email"]
  auto_verified_attributes = ["email"]
  deletion_protection      = "ACTIVE"
  mfa_configuration        = "ON"
  user_pool_tier           = "LITE"
  tags = {
    project     = "codexpt"
    environment = "oidc-loopback-test"
  }

  software_token_mfa_configuration {
    enabled = true
  }

  admin_create_user_config {
    allow_admin_create_user_only = true
  }

  password_policy {
    minimum_length                   = 12
    require_lowercase                = true
    require_numbers                  = true
    require_symbols                  = true
    require_uppercase                = true
    temporary_password_validity_days = 1
  }

  account_recovery_setting {
    recovery_mechanism {
      name     = "verified_email"
      priority = 1
    }
  }
}

resource "aws_cognito_user_pool_client" "codexpt_oidc_test" {
  name                                 = "codexpt-local-loopback-test"
  user_pool_id                         = aws_cognito_user_pool.codexpt_oidc_test.id
  generate_secret                      = false
  explicit_auth_flows                  = ["ALLOW_REFRESH_TOKEN_AUTH"]
  prevent_user_existence_errors        = "ENABLED"
  enable_token_revocation              = true
  allowed_oauth_flows_user_pool_client = true
  allowed_oauth_flows                  = ["code"]
  allowed_oauth_scopes                 = ["openid", "profile", "email"]
  read_attributes                      = ["email", "email_verified"]
  supported_identity_providers         = ["COGNITO"]
  callback_urls                        = ["http://127.0.0.1:4173/api/auth/callback"]
  logout_urls                          = ["http://127.0.0.1:4173/"]
  access_token_validity                = 1
  id_token_validity                    = 1
  refresh_token_validity               = 1

  token_validity_units {
    access_token  = "hours"
    id_token      = "hours"
    refresh_token = "days"
  }
}

resource "aws_cognito_user_pool_domain" "codexpt_oidc_test" {
  domain                = var.user_pool_domain_prefix
  managed_login_version = 1
  user_pool_id          = aws_cognito_user_pool.codexpt_oidc_test.id
}
