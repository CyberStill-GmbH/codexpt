data "aws_caller_identity" "current" {}

data "aws_ami" "al2023_arm64" {
  most_recent = true
  owners      = ["amazon"]

  filter {
    name   = "name"
    values = ["al2023-ami-2023.*-kernel-*-arm64"]
  }
  filter {
    name   = "architecture"
    values = ["arm64"]
  }
  filter {
    name   = "root-device-type"
    values = ["ebs"]
  }
  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }
}

resource "aws_cognito_user_pool" "codexpt" {
  count                    = var.identity_provider == "cognito" ? 1 : 0
  name                     = "codexpt-private-beta"
  username_attributes      = ["email"]
  auto_verified_attributes = ["email"]
  deletion_protection      = "ACTIVE"

  admin_create_user_config {
    allow_admin_create_user_only = true
  }

  password_policy {
    minimum_length                   = 12
    require_lowercase                = true
    require_numbers                  = true
    require_symbols                  = true
    require_uppercase                = true
    temporary_password_validity_days = 7
  }

  account_recovery_setting {
    recovery_mechanism {
      name     = "verified_email"
      priority = 1
    }
  }
}

resource "aws_cognito_user_pool_client" "codexpt" {
  count                                = var.identity_provider == "cognito" ? 1 : 0
  name                                 = "codexpt-web-bff"
  user_pool_id                         = aws_cognito_user_pool.codexpt[0].id
  generate_secret                      = false
  prevent_user_existence_errors        = "ENABLED"
  enable_token_revocation              = true
  allowed_oauth_flows_user_pool_client = true
  allowed_oauth_flows                  = ["code"]
  allowed_oauth_scopes                 = ["openid", "profile", "email"]
  supported_identity_providers         = ["COGNITO"]
  callback_urls                        = ["https://${var.api_domain}/api/auth/callback"]
  logout_urls                          = [var.web_origin]
  access_token_validity                = 1
  id_token_validity                    = 1
  refresh_token_validity               = 1
  token_validity_units {
    access_token  = "hours"
    id_token      = "hours"
    refresh_token = "days"
  }
}

resource "aws_cognito_user_pool_domain" "codexpt" {
  count        = var.identity_provider == "cognito" ? 1 : 0
  domain       = "${var.user_pool_domain_prefix}-${data.aws_caller_identity.current.account_id}"
  user_pool_id = aws_cognito_user_pool.codexpt[0].id
}

locals {
  oidc_issuer    = var.identity_provider == "openai" ? "https://auth.openai.com" : "https://${aws_cognito_user_pool.codexpt[0].endpoint}"
  oidc_client_id = var.identity_provider == "openai" ? var.openai_client_id : aws_cognito_user_pool_client.codexpt[0].id
}

resource "aws_s3_bucket" "backup" {
  bucket        = "codexpt-beta-backups-${data.aws_caller_identity.current.account_id}-${var.aws_region}"
  force_destroy = false
}

resource "aws_s3_bucket_public_access_block" "backup" {
  bucket                  = aws_s3_bucket.backup.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_versioning" "backup" {
  bucket = aws_s3_bucket.backup.id
  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "backup" {
  bucket = aws_s3_bucket.backup.id
  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "backup" {
  bucket = aws_s3_bucket.backup.id
  rule {
    id     = "expire-backups-and-old-versions"
    status = "Enabled"
    filter {}
    expiration {
      days = 30
    }
    noncurrent_version_expiration {
      noncurrent_days = 30
    }
  }
}

resource "aws_s3_bucket_policy" "backup_tls_only" {
  bucket = aws_s3_bucket.backup.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Sid       = "DenyInsecureTransport"
      Effect    = "Deny"
      Principal = "*"
      Action    = "s3:*"
      Resource = [
        aws_s3_bucket.backup.arn,
        "${aws_s3_bucket.backup.arn}/*"
      ]
      Condition = { Bool = { "aws:SecureTransport" = "false" } }
    }]
  })
  depends_on = [aws_s3_bucket_public_access_block.backup]
}

resource "aws_iam_role" "instance" {
  name = "codexpt-private-beta-instance"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "ec2.amazonaws.com" }
      Action    = "sts:AssumeRole"
    }]
  })
}

resource "aws_iam_role_policy_attachment" "ssm" {
  role       = aws_iam_role.instance.name
  policy_arn = "arn:${data.aws_partition.current.partition}:iam::aws:policy/AmazonSSMManagedInstanceCore"
}

data "aws_partition" "current" {}

resource "aws_iam_role_policy" "runtime" {
  name = "codexpt-private-beta-runtime"
  role = aws_iam_role.instance.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid      = "BackupDatabase"
        Effect   = "Allow"
        Action   = ["s3:PutObject", "s3:GetObject"]
        Resource = "${aws_s3_bucket.backup.arn}/daily/*"
      }
    ]
  })
}

resource "aws_iam_instance_profile" "instance" {
  name = "codexpt-private-beta-instance"
  role = aws_iam_role.instance.name
}

resource "aws_vpc" "beta" {
  cidr_block           = "10.72.0.0/16"
  enable_dns_support   = true
  enable_dns_hostnames = true
}

resource "aws_subnet" "public" {
  vpc_id                  = aws_vpc.beta.id
  cidr_block              = "10.72.1.0/24"
  availability_zone       = "${var.aws_region}a"
  map_public_ip_on_launch = false
}

resource "aws_internet_gateway" "beta" {
  vpc_id = aws_vpc.beta.id
}

resource "aws_route_table" "public" {
  vpc_id = aws_vpc.beta.id
  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.beta.id
  }
}

resource "aws_route_table_association" "public" {
  subnet_id      = aws_subnet.public.id
  route_table_id = aws_route_table.public.id
}

resource "aws_security_group" "api" {
  name        = "codexpt-private-beta-api"
  description = "Only HTTP redirect and HTTPS ingress; no SSH or database port."
  vpc_id      = aws_vpc.beta.id
  ingress {
    description = "ACME HTTP challenge and redirect"
    from_port   = 80
    to_port     = 80
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
  ingress {
    description = "HTTPS API"
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
  egress {
    description = "HTTPS to SSM, identity provider, S3, registries, packages and ACME"
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
  egress {
    description = "DNS queries to the VPC resolver"
    from_port   = 53
    to_port     = 53
    protocol    = "udp"
    cidr_blocks = ["10.72.0.0/16"]
  }
  egress {
    description = "Large DNS responses to the VPC resolver"
    from_port   = 53
    to_port     = 53
    protocol    = "tcp"
    cidr_blocks = ["10.72.0.0/16"]
  }
}

resource "aws_instance" "api" {
  ami                         = data.aws_ami.al2023_arm64.id
  instance_type               = var.instance_type
  subnet_id                   = aws_subnet.public.id
  vpc_security_group_ids      = [aws_security_group.api.id]
  iam_instance_profile        = aws_iam_instance_profile.instance.name
  associate_public_ip_address = false
  user_data_replace_on_change = false
  user_data = templatefile("${path.module}/templates/bootstrap.sh.tftpl", {
    api_domain        = var.api_domain
    app_release_url   = var.app_release_url
    app_release_hash  = lower(var.app_release_sha256)
    caddy_image       = var.caddy_image
    oidc_issuer       = local.oidc_issuer
    oidc_client_id    = local.oidc_client_id
    oidc_redirect_uri = "https://${var.api_domain}/api/auth/callback"
    login_success_url = "${var.web_origin}/"
    web_origin        = var.web_origin
    backup_bucket     = aws_s3_bucket.backup.bucket
    aws_region        = var.aws_region
  })

  lifecycle {
    precondition {
      condition     = var.identity_provider != "openai" || length(trimspace(var.openai_client_id)) > 0
      error_message = "OpenAI website sign-in cannot be planned without an OpenAI-issued public OAuth client ID."
    }
    precondition {
      condition     = data.aws_ami.al2023_arm64.architecture == "arm64"
      error_message = "CodeXPT beta requires an ARM64 Amazon Linux 2023 AMI."
    }
  }

  metadata_options {
    http_endpoint = "enabled"
    http_tokens   = "required"
  }
  credit_specification {
    cpu_credits = "standard"
  }
  root_block_device {
    encrypted             = true
    volume_type           = "gp3"
    volume_size           = var.root_volume_gib
    delete_on_termination = true
  }

  depends_on = [
    aws_route_table_association.public,
    aws_iam_role_policy_attachment.ssm,
    aws_iam_role_policy.runtime,
  ]
}

resource "aws_eip" "api" {
  domain = "vpc"
}

resource "aws_eip_association" "api" {
  instance_id   = aws_instance.api.id
  allocation_id = aws_eip.api.id
}

resource "aws_budgets_budget" "monthly" {
  name         = "codexpt-private-beta-monthly-alert"
  budget_type  = "COST"
  limit_amount = tostring(var.monthly_budget_usd)
  limit_unit   = "USD"
  time_unit    = "MONTHLY"

  notification {
    comparison_operator        = "GREATER_THAN"
    threshold                  = 50
    threshold_type             = "PERCENTAGE"
    notification_type          = "ACTUAL"
    subscriber_email_addresses = [var.budget_alert_email]
  }
  notification {
    comparison_operator        = "GREATER_THAN"
    threshold                  = 80
    threshold_type             = "PERCENTAGE"
    notification_type          = "FORECASTED"
    subscriber_email_addresses = [var.budget_alert_email]
  }
}
