output "hub_url" {
  description = "The hub's address: open it in a browser for first-run setup, and give it to schools."
  value       = google_cloud_run_v2_service.hub.uri
}

output "database_instance" {
  description = "Cloud SQL instance holding every school's synced copy. Back it up; see docs/nexus/deploy.md."
  value       = google_sql_database_instance.db.connection_name
}

output "image" {
  description = "The image the hub runs, through the Artifact Registry cache of ghcr.io."
  value       = local.image
}
