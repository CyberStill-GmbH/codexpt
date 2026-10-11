locals {
  worker_bundle = abspath("${path.module}/${var.worker_bundle_path}")
  api_host      = "${var.worker_name}.${var.workers_dev_subdomain}.workers.dev"
}

resource "cloudflare_d1_database" "api" {
  account_id            = var.account_id
  name                  = var.database_name
  primary_location_hint = "wnam"

  lifecycle {
    prevent_destroy = true
  }
}

resource "cloudflare_workers_script" "api" {
  account_id         = var.account_id
  script_name        = var.worker_name
  content_file       = local.worker_bundle
  content_sha256     = filesha256(local.worker_bundle)
  main_module        = "worker.js"
  compatibility_date = var.compatibility_date
  usage_model        = "standard"

  bindings = [
    {
      name = "DB"
      type = "d1"
      id   = cloudflare_d1_database.api.id
    },
    {
      name = "API_HOST"
      type = "plain_text"
      text = local.api_host
    },
    {
      name = "WEB_ORIGIN"
      type = "plain_text"
      text = var.web_origin
    },
    {
      name = "OIDC_ISSUER"
      type = "plain_text"
      text = var.oidc_issuer
    },
    {
      name = "OIDC_CLIENT_ID"
      type = "plain_text"
      text = var.oidc_client_id
    },
    {
      name = "OIDC_REDIRECT_URI"
      type = "plain_text"
      text = "${var.web_origin}/api/auth/callback"
    },
    {
      name = "LOGIN_SUCCESS_URL"
      type = "plain_text"
      text = "${var.web_origin}/"
    },
  ]

  lifecycle {
    prevent_destroy = true

    precondition {
      condition     = var.production_release_ready && var.oidc_issuer != "" && var.oidc_client_id != ""
      error_message = "Production deploy is blocked until API parity, identity, tenant isolation, recovery, and hosted-browser release gates pass and the dedicated OIDC issuer/client ID are configured."
    }
  }
}

resource "cloudflare_workers_script_subdomain" "api" {
  account_id       = var.account_id
  script_name      = cloudflare_workers_script.api.script_name
  enabled          = true
  previews_enabled = false
}
