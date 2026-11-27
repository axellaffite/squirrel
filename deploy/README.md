# Deploying Squirrel on a home server

Target: a small x86-64 Linux box (built for an Intel NUC 12 Pro: i3-1220P, 8 GB RAM, NVMe),
reached from phones and browsers over [Tailscale](https://tailscale.com). The app is never exposed
to the internet: the server listens on `127.0.0.1:8080` and `tailscale serve` adds HTTPS for the
devices of your tailnet. Public links, if you turn them on, have a listener of their own
(`127.0.0.1:8081`) that serves nothing else, exposed with Tailscale Funnel (see "Public links").

What runs (`deploy/docker-compose.yml`):

| Service | What | Notes |
|---|---|---|
| `postgres` | Postgres 17 + pgvector 0.8.7 (pinned by digest) | Owner role is a superuser, so migrations create `vector` |
| `server` | `squirrel serve`: API, web UI, search | Migrates on start; published on `127.0.0.1:8080` (and `127.0.0.1:8081` for public links) |
| `worker` | `squirrel worker`: thumbnails, metadata, text, ML | Waits for the server's migrations |

The image (`Dockerfile`) holds the binary with the web UI, libheif, libvips (lightbox previews),
pdfium, ONNX Runtime, the SigLIP2 and face models, and the GeoNames snapshot; downloads are
pinned by SHA-256 at build time, and the build fails if libvips cannot load HEIF.
Nothing is downloaded at run time. The service runs as uid 10001; models and libraries are
root-owned and read-only.

## First install

On the server (Debian or Ubuntu with SSH access):

1. Docker Engine with the compose plugin: <https://docs.docker.com/engine/install/>, then
   `sudo usermod -aG docker $USER` (log in again).
2. Tailscale: `curl -fsSL https://tailscale.com/install.sh | sh && sudo tailscale up`.
   In the admin console, enable MagicDNS and HTTPS certificates.
3. Directories on the NVMe drive. The container runs as uid 10001.
   - Rootful Docker (`docker info` shows no `rootless`):

         sudo install -d -o 10001 -g 10001 -m 0750 /srv/squirrel/data
         sudo install -d -m 0700 /srv/squirrel/postgres

   - Rootless Docker: container uids map into your subuid range, so use directories you own
     and set `SQUIRREL_ROOT` to their parent in step 4 (no sudo needed):

         mkdir -p ~/squirrel-data/data ~/squirrel-data/postgres
         docker run --rm -v ~/squirrel-data:/r alpine chown 10001:10001 /r/data

     Rootless Docker is a service of your user: let it start at boot without a login with
     `loginctl enable-linger $USER`.

4. Settings: `mkdir ~/squirrel`, copy `deploy/env.example` to `~/squirrel/.env`,
   `chmod 600 ~/squirrel/.env`, and fill in `POSTGRES_PASSWORD` (`openssl rand -hex 24`) and
   `SQUIRREL_SETUP_TOKEN` (`openssl rand -hex 16`).

From the development machine (Docker with buildx):

5. `deploy/ship.sh <ssh-host>`: builds the image for linux/amd64 (about 20–40 minutes on an
   Apple Silicon Mac, under emulation), copies it over SSH, installs the compose file and
   starts everything. Later releases use the same command.

Back on the server:

6. HTTPS for the tailnet: `tailscale serve --bg 8080` (once `sudo tailscale set
   --operator=$USER` lets you run it without sudo). The address is
   `https://<machine>.<tailnet>.ts.net` (`tailscale serve status` prints it). If another
   program already listens on port 443, serve on another one: `tailscale serve --bg
   --https=8443 8080` gives `https://<machine>.<tailnet>.ts.net:8443`.
7. Open that address, enter the setup code from `.env`, and create the administrator.
   Everyone else joins with an invitation link. In the phone app, use the same address.

The first start computes the tag label bank (a few minutes) and then processes the library in
the background: thumbnails first, then tags, search embeddings and faces.

## Public links (optional)

A public link lets anyone with the link see one folder, file or album, without an account
(Share dialog, "Public link"). Links can have an expiry date and a password, and are revoked
from the same dialog.

The server answers public links on a second listener, `127.0.0.1:8081`, which serves only the
link pages, their API (`/api/public/…`) and the web assets (the app's script files, which the
link pages load, hold no data): the login page, the rest of the API and every private page answer
404 there. [Tailscale Funnel](https://tailscale.com/kb/1223/funnel)
puts it on the internet, on the same machine name as the tailnet address:

1. In the Tailscale admin console, allow Funnel for the server (Access controls: the `funnel`
   node attribute; the console offers to add it the first time you run the command below).
2. `tailscale funnel --bg --https=10000 8081`. Funnel only uses ports 443, 8443 and 10000; step 6
   already serves the app on 443 or 8443, and a Funnel on that port would replace it, so 10000.
   `tailscale funnel status` shows it.
3. In `~/squirrel/.env`, set `SQUIRREL_PUBLIC_URL=https://<machine>.<tailnet>.ts.net:10000`
   (the Funnel address), then `docker compose up -d`. The Share dialog now makes links.

To turn them off: `tailscale funnel --https=10000 off` (links stop opening from outside the
tailnet) and unset `SQUIRREL_PUBLIC_URL` (no new links). Links already made keep working on the
tailnet address until they are revoked or expire.

Good to know:

- A link's address is shown once, when it is made; the server keeps only a hash of it.
- Visitors can download whatever the link opens: what a browser shows can be saved.
- Wrong passwords are rate limited per visitor address, like logins, and checked one at a time
  apart from logins, so visitors never slow down your own sign-ins.
- With `RUST_LOG=debug`, request logs contain link tokens (`/api/public/<token>/…`).

## Operating

    cd ~/squirrel
    docker compose ps                         # state of the three services
    docker compose logs -f --tail=100 worker  # JSON logs
    docker compose exec server squirrel admin set-quota alice 10737418240
    docker compose exec server squirrel admin jobs

- **Update:** `deploy/ship.sh <ssh-host>` again. **Rollback:** set `SQUIRREL_TAG` in `.env` to
  an earlier tag (`docker images squirrel`), then `docker compose up -d`.
- **Restart policy:** every service restarts unless stopped; Docker starts them at boot.
- **Backups:** the database and `/srv/squirrel/data` must be backed up together (see the
  `docs/configuration.md`, "Data directory"). Automated backups are GitHub issue #13.
- **Memory (8 GB):** Postgres, the worker's ML child (about 0.6 GB while photos are being
  processed), the server's search model (about 1.9 GB while people search, stopped after
  5 idle minutes) and up to two decoding children (2 GiB address space each). The first start
  also runs the label bank's text model (about 1.9 GB, once).
- **Licences:** the image bundles third-party libraries under their own licences (pdfium BSD,
  ONNX Runtime MIT, libheif and libvips LGPL, models: see `crates/squirrel-ml/MODELS.md`).
  Debian's libvips also pulls in ImageMagick and poppler (GPL); Squirrel never lets libvips use
  those loaders (only JPEG, PNG, WebP, TIFF, GIF and HEIF). The image is built for your own
  server, not for redistribution.
