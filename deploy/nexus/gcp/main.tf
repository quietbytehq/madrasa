# Madrasa Nexus hub on Google Cloud: one Cloud Run service, Cloud SQL for PostgreSQL and
# Secret Manager.
#
#   cp terraform.tfvars.example terraform.tfvars   # then edit
#   terraform init
#   terraform apply
#
# See docs/nexus/deploy.md. This module is checked by CI (terraform validate) but has not
# been applied by the project: treat the first deployment as a test.
#
# The Terraform state holds the database password. Keep the state private (a bucket with
# restricted access, or a local file that is not shared or committed).

locals {
  hub_port        = 4780
  database_name   = "nexus"
  database_user   = "nexus"
  repository_id   = "${var.name}-ghcr"
  image           = "${var.region}-docker.pkg.dev/${var.project_id}/${local.repository_id}/${var.image_path}:${var.image_tag}"
  with_setup_token = var.setup_token != ""
}

resource "google_project_service" "apis" {
  for_each = toset([
    "artifactregistry.googleapis.com",
    "run.googleapis.com",
    "secretmanager.googleapis.com",
    "sqladmin.googleapis.com",
  ])

  service            = each.value
  disable_on_destroy = false
}

# Cloud Run pulls images from Artifact Registry. This repository is a pull-through cache of
# GitHub Container Registry, where the project publishes the hub image.
resource "google_artifact_registry_repository" "ghcr" {
  location      = var.region
  repository_id = local.repository_id
  description   = "Pull-through cache of ghcr.io for the Madrasa Nexus hub image"
  format        = "DOCKER"
  mode          = "REMOTE_REPOSITORY"

  remote_repository_config {
    description = "GitHub Container Registry"

    docker_repository {
      custom_repository {
        uri = "https://ghcr.io"
      }
    }
  }

  depends_on = [google_project_service.apis]
}

# ---- database ---------------------------------------------------------------------------------

# Letters and digits only: the password becomes part of a connection URL.
resource "random_password" "database" {
  length  = 32
  special = false
}

resource "google_sql_database_instance" "db" {
  name                = "${var.name}-db"
  region              = var.region
  database_version    = "POSTGRES_16"
  deletion_protection = var.deletion_protection

  settings {
    tier              = var.db_tier
    edition           = "ENTERPRISE"
    availability_type = "ZONAL"
    disk_size         = var.db_disk_gb
    disk_autoresize   = true

    backup_configuration {
      enabled                        = true
      point_in_time_recovery_enabled = true
      start_time                     = "21:00"
    }

    # A public address with no authorised networks: nothing can connect to it directly. The
    # hub connects through Cloud Run's built-in Cloud SQL connection, which is encrypted and
    # checked by IAM.
    ip_configuration {
      ipv4_enabled = true
      ssl_mode     = "ENCRYPTED_ONLY"
    }
  }

  depends_on = [google_project_service.apis]
}

resource "google_sql_database" "nexus" {
  name     = local.database_name
  instance = google_sql_database_instance.db.name
}

resource "google_sql_user" "nexus" {
  name     = local.database_user
  instance = google_sql_database_instance.db.name
  password = random_password.database.result
}

# ---- secrets ----------------------------------------------------------------------------------

resource "google_secret_manager_secret" "database_url" {
  secret_id = "${var.name}-database-url"

  replication {
    auto {}
  }

  depends_on = [google_project_service.apis]
}

resource "google_secret_manager_secret_version" "database_url" {
  secret = google_secret_manager_secret.database_url.id
  # The hub reaches the database through the socket Cloud Run mounts under /cloudsql.
  secret_data = "postgres://${local.database_user}:${random_password.database.result}@localhost/${local.database_name}?host=/cloudsql/${google_sql_database_instance.db.connection_name}"
}

resource "google_secret_manager_secret" "setup_token" {
  count     = local.with_setup_token ? 1 : 0
  secret_id = "${var.name}-setup-token"

  replication {
    auto {}
  }

  depends_on = [google_project_service.apis]
}

resource "google_secret_manager_secret_version" "setup_token" {
  count       = local.with_setup_token ? 1 : 0
  secret      = google_secret_manager_secret.setup_token[0].id
  secret_data = var.setup_token
}

# ---- the hub ----------------------------------------------------------------------------------

# The hub runs as its own service account, which may read its two secrets and connect to its
# database, and nothing else.
resource "google_service_account" "hub" {
  account_id   = "${var.name}-hub"
  display_name = "Madrasa Nexus hub"
}

resource "google_project_iam_member" "hub_sql" {
  project = var.project_id
  role    = "roles/cloudsql.client"
  member  = "serviceAccount:${google_service_account.hub.email}"
}

resource "google_secret_manager_secret_iam_member" "hub_database_url" {
  secret_id = google_secret_manager_secret.database_url.id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.hub.email}"
}

resource "google_secret_manager_secret_iam_member" "hub_setup_token" {
  count     = local.with_setup_token ? 1 : 0
  secret_id = google_secret_manager_secret.setup_token[0].id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.hub.email}"
}

resource "google_cloud_run_v2_service" "hub" {
  name                = var.name
  location            = var.region
  ingress             = "INGRESS_TRAFFIC_ALL"
  deletion_protection = false

  template {
    service_account = google_service_account.hub.email

    # The alpha hub runs as exactly one instance: it keeps a clock of its own, and two
    # instances would share it.
    scaling {
      min_instance_count = var.min_instances
      max_instance_count = 1
    }

    volumes {
      name = "cloudsql"

      cloud_sql_instance {
        instances = [google_sql_database_instance.db.connection_name]
      }
    }

    containers {
      image = local.image

      # Cloud Run passes this port as PORT, which the hub listens on.
      ports {
        container_port = local.hub_port
      }

      resources {
        limits = {
          cpu    = "1"
          memory = "1Gi"
        }
      }

      env {
        name = "NEXUS_DATABASE_URL"

        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.database_url.secret_id
            version = "latest"
          }
        }
      }

      dynamic "env" {
        for_each = local.with_setup_token ? [1] : []

        content {
          name = "NEXUS_SETUP_TOKEN"

          value_source {
            secret_key_ref {
              secret  = google_secret_manager_secret.setup_token[0].secret_id
              version = "latest"
            }
          }
        }
      }

      dynamic "env" {
        for_each = var.public_url != "" ? [1] : []

        content {
          name  = "NEXUS_PUBLIC_URL"
          value = var.public_url
        }
      }

      volume_mounts {
        name       = "cloudsql"
        mount_path = "/cloudsql"
      }

      startup_probe {
        period_seconds    = 5
        failure_threshold = 30

        http_get {
          path = "/api/nexus/v1/health"
        }
      }

      liveness_probe {
        period_seconds = 30

        http_get {
          path = "/api/nexus/v1/health"
        }
      }
    }
  }

  depends_on = [
    google_artifact_registry_repository.ghcr,
    google_project_iam_member.hub_sql,
    google_secret_manager_secret_iam_member.hub_database_url,
    google_secret_manager_secret_iam_member.hub_setup_token,
    google_secret_manager_secret_version.database_url,
    google_secret_manager_secret_version.setup_token,
    google_sql_database.nexus,
    google_sql_user.nexus,
  ]
}

# Schools and head office reach the hub over the internet; the hub checks every request
# itself (sync tokens for schools, sign-in for the console).
resource "google_cloud_run_v2_service_iam_member" "public" {
  name     = google_cloud_run_v2_service.hub.name
  location = google_cloud_run_v2_service.hub.location
  role     = "roles/run.invoker"
  member   = "allUsers"
}
