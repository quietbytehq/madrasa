# Deploying the Madrasa Nexus hub

The hub is one container (`madrasa-nexus`) and a PostgreSQL 16 database. Schools reach it
over HTTPS; it never calls a school.

What has been verified, and how:

| Way to deploy | Status |
| --- | --- |
| Docker image and Docker Compose | Built and started by CI on every change (health, the web console, first-run setup and login over HTTP). The hub and its console were also run by hand on a development machine with a demo school joined to it. |
| Azure, Google Cloud | Templates arrive in stage 8. |

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
