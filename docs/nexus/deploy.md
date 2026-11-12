# Deploying the Madrasa Nexus hub

The hub is one container (`madrasa-nexus`) and a PostgreSQL 16 database. Schools reach it
over HTTPS; it never calls a school.

What has been verified, and how:

| Way to deploy | Status |
| --- | --- |
| Docker image and Docker Compose | Built and started by CI on every change (health, the web console, first-run setup and login over HTTP). The hub and its console were also run by hand on a development machine with a demo school joined to it. |
| Azure (Bicep) | Checked for errors by CI on every change (`az bicep build`). **Not deployed by the project**: no Azure subscription was available. Treat your first deployment as a test. |
| Google Cloud (Terraform) | Checked for errors by CI on every change (`terraform validate`). **Not applied by the project**: no Google Cloud project was available. Treat your first deployment as a test. |

## Docker Compose (one machine)

You need Docker with the Compose plugin and a machine that schools can reach.

```bash
cd deploy/nexus
```

```bash
cp .env.example .env
```

Edit `.env`:

- `NEXUS_DB_PASSWORD`: a long random password of letters and digits (`openssl rand -hex 24`).
- `NEXUS_PUBLIC_URL`: the address schools will type, for example `https://nexus.example.org`.
- `NEXUS_DOMAIN`: the same host name without `https://`, if you use the bundled HTTPS proxy.

Start it. Without the `https` profile the hub listens on this machine only
(`http://127.0.0.1:4780`), which is right for a first look or behind your own reverse proxy:

```bash
docker compose up -d
```

For a real network, let the bundled Caddy obtain a certificate and serve HTTPS (the domain
must point at this machine, and ports 80 and 443 must be open):

```bash
docker compose --profile https up -d
```

To build the image from this checkout instead of pulling a release:

```bash
docker compose build nexus
```

### First run

The hub prints a one-time setup token when it starts with an empty database:

```bash
docker compose logs nexus | grep "setup token"
```

Open the hub's address in a browser (`http://127.0.0.1:4780`, or your `https://` address).
The first visit shows the setup page: paste the token, name the network and create the first
administrator. You may also choose the escrow passphrase there (see below). Then sign in.

The same can be done without a browser (replace the values):

```bash
curl -X POST http://127.0.0.1:4780/api/nexus/v1/setup -H "content-type: application/json" -d '{"setup_token":"PASTE-TOKEN","hub_name":"My School Network","admin_username":"head.office","admin_full_name":"Head Office","admin_password":"choose-a-long-password"}'
```

The token stops working as soon as setup succeeds. To choose the token yourself, set
`NEXUS_SETUP_TOKEN` in `.env` before the first start.

### Trying it with made-up schools

To look around before any real school joins, fill the **empty** hub with a demo network of
three generated schools (one bills in another currency):

```bash
docker compose run --rm nexus demo-network --yes
```

Nothing in it is real, and no installation can sync as a demo school. When you are done,
remove each demo school in the console and delete its archive. The command refuses to run
on a hub that already has schools.

### The console

The hub serves its own web console at its address: schools and invitations, sync state and
the overwrite log, network-wide reports with Excel and CSV downloads, corrections to a
school's names and contact details, backup key escrow, head office users and the activity
log. English and Urdu. Each head office user sees only the pages their permissions open,
and the hub checks every permission again itself.

Signing in lasts for the browser tab: closing it signs out. Use the console over HTTPS only
(or on the hub machine itself for a first look).

### Settings

| Variable | Meaning |
| --- | --- |
| `NEXUS_DATABASE_URL` | PostgreSQL URL of the hub database (Compose builds it from `NEXUS_DB_PASSWORD`) |
| `NEXUS_BIND` | Address to listen on inside the container, default `0.0.0.0:4780` |
| `PORT` | Used instead of `NEXUS_BIND` when set (Cloud Run, Azure Container Apps) |
| `NEXUS_PUBLIC_URL` | The address schools use; shown in invitations |
| `NEXUS_SETUP_TOKEN` | One-time token for first-run setup; generated and logged when empty |
| `NEXUS_CONSOLE_DIR` | Folder of the web console's files; the image sets it to `/app/console`. Without it the hub serves the API only |
| `RUST_LOG` | Log level, default `info` |

No secret is written to disk by the hub and none is logged, except the generated setup
token, once, which is useless after setup.

### Looking after it

- **Back up the database volume** (`nexus-db`). It holds every school's synced copy.
  `docker compose exec db pg_dump -U nexus nexus > nexus-backup.sql` makes a dump.
- **Upgrade** by pulling a newer image and running `docker compose up -d`; the hub applies
  its migrations when it starts.
- **Health**: `GET /api/nexus/v1/health`; the container reports healthy or unhealthy by
  itself (`docker compose ps`).

## The image

The release workflow builds the image (hub and web console) and publishes it to GitHub
Container Registry when a release is tagged:

- `ghcr.io/quietbytehq/madrasa-nexus:<version>` (the tag without its `v`, for example `:0.4.1-alpha`)
- `ghcr.io/quietbytehq/madrasa-nexus:latest`

Two things to know before the first cloud deployment:

- **No image has been published yet.** The first one appears with the next tagged release.
  Until then, build it from a checkout (`docker compose build nexus`).
- **GitHub makes a new package private.** After the first publish, an owner of the
  repository must set the package's visibility to public (GitHub → Packages →
  madrasa-nexus → Package settings), or Azure and Google Cloud cannot pull it.

Deploy a version tag, not `latest`, so that an upgrade is something you decide.

## Azure (Container Apps and PostgreSQL Flexible Server)

`deploy/nexus/azure/main.bicep` creates a Log Analytics workspace, a Container Apps
environment, the hub as one Container App with HTTPS ingress, and an Azure Database for
PostgreSQL Flexible Server (version 16) with a database named `nexus`. The database URL and
the setup token are Container App secrets.

**Not deployed by the project.** CI checks that the template builds; nobody has run it
against a subscription. Read the plan Azure shows before you confirm.

You need the Azure CLI, signed in (`az login`), and a subscription.

```bash
az group create --name madrasa-nexus --location centralindia
```

```bash
export NEXUS_DB_PASSWORD="$(openssl rand -hex 24)"
```

```bash
cd deploy/nexus/azure
```

Edit `main.example.bicepparam` (or a copy named `main.bicepparam`, which Git ignores): set
`image` to a published release. Then see what would be created:

```bash
az deployment group what-if --resource-group madrasa-nexus --template-file main.bicep --parameters main.example.bicepparam
```

```bash
az deployment group create --resource-group madrasa-nexus --template-file main.bicep --parameters main.example.bicepparam
```

The deployment prints `hubUrl`. Open it in a browser for first-run setup. The setup token is
in the hub's log:

```bash
az containerapp logs show --resource-group madrasa-nexus --name madrasa-nexus --tail 50
```

Keep the database password: it is not shown again. Things this template decides for you:

- The hub runs as exactly **one** instance (the alpha hub must not run as several).
- The database has a public endpoint that accepts connections **from Azure services only**
  and only over TLS with the password. For a private endpoint, put the server and the
  Container Apps environment in a virtual network (not in this template).
- Azure keeps automatic database backups for 14 days (`dbBackupRetentionDays`).
- To use your own domain, add it to the Container App in the Azure portal, then deploy again
  with `publicUrl` set so invitations show it.

## Google Cloud (Cloud Run, Cloud SQL and Secret Manager)

`deploy/nexus/gcp` is a Terraform module: a Cloud Run service for the hub, a Cloud SQL for
PostgreSQL 16 instance with a database named `nexus`, the database URL and the setup token
in Secret Manager, a service account for the hub that may read those secrets and connect to
that database, and an Artifact Registry repository that fetches the image from GitHub
Container Registry (Cloud Run cannot pull from it directly).

**Not applied by the project.** CI checks that the module is valid; nobody has run it
against a project. Read the plan Terraform shows before you confirm.

You need Terraform 1.5 or newer, the gcloud CLI signed in
(`gcloud auth application-default login`), and a project with billing.

```bash
cd deploy/nexus/gcp
```

```bash
cp terraform.tfvars.example terraform.tfvars
```

Edit `terraform.tfvars` (project, region, image tag), then:

```bash
terraform init
```

```bash
terraform plan
```

```bash
terraform apply
```

Terraform prints `hub_url`. Open it in a browser for first-run setup. The setup token is in
the hub's log:

```bash
gcloud run services logs read madrasa-nexus --region YOUR-REGION --limit 50
```

Things this module decides for you:

- The hub runs as at most **one** instance. With `min_instances = 0` (the default) Cloud Run
  stops it when nobody uses it and the next request takes a few seconds; set `1` to keep it
  running.
- The database password is generated by Terraform and is in the **Terraform state**. Keep the
  state private: a storage bucket with restricted access, or a local file you do not share
  or commit (Git ignores `*.tfstate` and `terraform.tfvars` here).
- The database has no authorised networks: only the hub's Cloud SQL connection reaches it.
  Automatic backups and point-in-time recovery are on. `deletion_protection` is on, so
  `terraform destroy` refuses to delete the database until you turn it off.
- The Cloud Run service is public (`allUsers` may call it): schools must reach it, and the
  hub checks every request itself.

## Operations

**Back up the hub database.** It holds every school's synced copy and the hub's own records.
Schools keep their own data and their own backups; the hub database is head office's copy.

- Compose: `docker compose exec db pg_dump -U nexus nexus > nexus-backup.sql`, on a schedule,
  kept on another machine.
- Azure and Google Cloud: the managed database's automatic backups (14 days on Azure by this
  template; daily with point-in-time recovery on Google Cloud). Try a restore once before
  you rely on them.
- If the hub database is lost and cannot be restored, no school loses anything: set up a new
  hub, invite each school again, and each uploads its records again. Backup keys left in
  escrow are gone and must be left again.

**The escrow passphrase** is not in any backup, by design. Keep it written down, away from
the server.

**Upgrade.** Deploy the newer image tag (Compose: pull and `docker compose up -d`; Azure:
deploy again with the new `image`; Google Cloud: change `image_tag` and `terraform apply`).
The hub applies its own migrations and each school's when it starts. Upgrade the hub
**before** the schools: a school on a newer version than its hub is told the hub must be
upgraded first, and waits.

**If a school's sync key may have leaked.** There is no way yet to replace a school's key
in place. Remove the school at the hub (Schools → Open → Remove from the network): its key
stops working at once. Then invite it again; it joins with a new key and uploads its records
again into a new copy. Delete the old archive when you no longer need it.

**If a head office password may have leaked.** An administrator sets a new one under Users;
that ends the user's sessions. Disable accounts that are no longer needed.

**Watching it.** `GET /api/nexus/v1/health` answers `200` when the hub and its database are
well. The Schools page shows when each school last synced; a school that has been silent
for days deserves a call. The Activity log shows everything done at the hub.

## Backup key escrow (optional)

Schools may leave a sealed copy of their backup key with head office, for the day a school
loses both its backup password and its recovery key (docs/spec.md §16.7). To allow it,
choose an **escrow passphrase** of at least 12 characters: on the setup page, or later under
**Backup keys** in the console (an administrator).

Write the passphrase down and keep it somewhere safe, away from the server. The hub does
not store it. Without it no key can be released, and nobody can recover it for you.
Releasing a key (Backup keys → Release key) needs a hub user with the `nexus.escrow`
permission, the passphrase and a reason; the school is told at its next sync. The released
code is shown once; give it to the school by a safe route.

## Adding a school

1. Head office opens **Schools → Invite a school** in the console and types the school's
   name. The console shows a join code once; it works once and expires after seven days.
2. At the school, a user who manages settings opens Settings → School network (Nexus),
   enters the hub address and the join code, and joins. The first upload starts at once.
3. The school syncs every five minutes and when someone presses Sync now. It keeps working
   when the hub or the internet is away and catches up afterwards.

The hub address must start with `https://`. A school refuses to send its records over plain
`http://`, except to a hub on the same computer (`http://localhost:4780`), which is only for
trying things out.
