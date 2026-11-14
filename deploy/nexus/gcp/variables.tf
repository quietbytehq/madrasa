variable "project_id" {
  description = "Google Cloud project to deploy into."
  type        = string
}

variable "region" {
  description = "Region for Cloud Run, Cloud SQL and the image repository, for example asia-south1."
  type        = string
}

variable "name" {
  description = "Short name used for every resource: lower-case letters, digits and hyphens."
  type        = string
  default     = "madrasa-nexus"

  validation {
    condition     = can(regex("^[a-z][a-z0-9-]{2,20}$", var.name))
    error_message = "Use 3 to 21 lower-case letters, digits or hyphens, starting with a letter."
  }
}

variable "image_tag" {
  description = "Release tag of the hub image (ghcr.io/quietbytehq/madrasa-nexus), for example 0.5.0-alpha (release tags end in -alpha). Avoid \"latest\": an upgrade should be a decision."
  type        = string
}

variable "image_path" {
  description = "Image path on GitHub Container Registry, without the registry host and the tag."
  type        = string
  default     = "quietbytehq/madrasa-nexus"
}

variable "public_url" {
  description = "The address schools type, when you put your own domain in front (https://nexus.example.org). Empty: invitations show the address the console is opened at."
  type        = string
  default     = ""
}

variable "setup_token" {
  description = "One-time token for first-run setup. Empty: the hub generates one and prints it once in its log."
  type        = string
  default     = ""
  sensitive   = true
}

variable "db_tier" {
  description = "Cloud SQL machine type. db-custom-1-3840 (1 vCPU, 3.75 GB) suits a first network; db-g1-small is cheaper for a trial."
  type        = string
  default     = "db-custom-1-3840"
}

variable "db_disk_gb" {
  description = "Cloud SQL disk size in GB (grows automatically)."
  type        = number
  default     = 20
}

variable "min_instances" {
  description = "Hub instances kept running. 0 lets Cloud Run stop the hub when nobody uses it (the first request then takes a few seconds); 1 keeps it warm."
  type        = number
  default     = 0

  validation {
    condition     = contains([0, 1], var.min_instances)
    error_message = "The alpha hub runs as one instance: use 0 or 1."
  }
}

variable "deletion_protection" {
  description = "Refuse to delete the database with terraform destroy. Leave on for anything real."
  type        = bool
  default     = true
}
