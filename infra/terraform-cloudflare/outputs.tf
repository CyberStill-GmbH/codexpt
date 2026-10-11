output "worker_name" {
  description = "Deployed Cloudflare Worker script name."
  value       = cloudflare_workers_script.api.script_name
}

output "api_url" {
  description = "Provider-generated HTTPS hostname; custom jappi.net.pe DNS is intentionally unmanaged."
  value       = "https://${var.worker_name}.${var.workers_dev_subdomain}.workers.dev"
}

output "database_name" {
  description = "D1 database managed by this Terraform state."
  value       = cloudflare_d1_database.api.name
}

output "database_id" {
  description = "D1 database ID to configure in the Worker binding and deployment tooling."
  value       = cloudflare_d1_database.api.id
}
