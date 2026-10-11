terraform {
  required_version = ">= 1.8.0"

  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 5.27"
    }
  }
}

provider "cloudflare" {
  # Authentication is read from CLOUDFLARE_API_TOKEN. Never put API tokens
  # in Terraform variables, tfvars files, or state.
}
