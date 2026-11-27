# Configuration and operations

Everything a running Squirrel reads from its environment, where it keeps its data, how to
administer it, and how media processing and machine learning work. Installing on a home server:
[`deploy/README.md`](../deploy/README.md). Working on the code: [`development.md`](development.md).

## Server settings

| Variable | Default | Purpose |
| --- | --- | --- |
| `SQUIRREL_DATABASE_URL` | (required) | Postgres 17 with pgvector 0.8 (see below). Also read by `squirrel migrate` and `squirrel admin` |
| `SQUIRREL_DATA_DIR` | (required) | Where file contents are stored (`blobs/`, `tmp/`). Created if missing; `tmp/` and `blobs/` must be on the same filesystem |
| `SQUIRREL_BIND` | `0.0.0.0:8080` | Listen address |
| `SQUIRREL_COOKIE_SECURE` | `true` | Set to `false` only for local HTTP development |
| `SQUIRREL_SETUP_TOKEN` | generated and logged | Setup code for the first administrator |
| `SQUIRREL_PUBLIC_URL` | unset | Base address of public links, e.g. `https://home.tail1234.ts.net:10000`. Unset or empty: links cannot be made (see `deploy/README.md`, "Public links") |
| `SQUIRREL_PUBLIC_BIND` | unset | Second listen address serving public links only (link pages, `/api/public/…`, web assets), e.g. `127.0.0.1:8081` |
| `SQUIRREL_TRUSTED_PROXIES` | empty | Comma-separated IPs/CIDRs of reverse proxies whose `X-Forwarded-For` is trusted for rate limiting and logs. Leave empty when clients connect directly |
| `SQUIRREL_LOG_JSON` | `false` | JSON logs |
| `SQUIRREL_PREVIEW_CACHE_MB` | `1536` | Disk budget of the photo previews rendered on request (`cache/previews/`), least recently used first out |
| `SQUIRREL_PREVIEW_CONCURRENCY` | `2` | Photo previews rendered at once; a request waiting more than 3 s for a slot gets 503 `busy` (`Retry-After: 1`) |
| `RUST_LOG` | `info,squirrel=debug,tower_http=info` | Log filter ([`tracing` `EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) syntax) |

Each variable also exists as a `serve` flag (`--data-dir`, `--bind`, …; see `squirrel serve --help`).
Upload, quota and trash behaviour is not configurable yet; the built-in values are:

| Setting | Value |
| --- | --- |
| Upload chunk size | 8 MiB |
| Unfinished uploads expire after | 24 h without activity |
| Free disk space kept for the system | 1 GiB (uploads that would go below it are refused) |
| Trash retention | 30 days, then deleted for good |
| Unreferenced file contents removed after | 24 h |
| Storage quota | unlimited per user unless set with `squirrel admin set-quota` |
| Space one user's open uploads may reserve | half the usable free space (their only open upload may use all of it) |

These become configurable in Phase 7.

## Deployment model

Production runs from one Docker image (`Dockerfile`) with Postgres, the server and the worker
in `deploy/docker-compose.yml`; `deploy/README.md` is the install guide (a home server reached
over Tailscale).

Squirrel assumes **one server process per database and data directory**. Upload locks,
upload admission and rate limits live in that process's memory; two servers sharing a
database or a data directory would not see each other's locks.

## Data directory

    <SQUIRREL_DATA_DIR>/
      .squirrel-instance     id of the installation (database) this directory belongs to
      blobs/ab/cd/<sha256>   file contents, stored once per distinct content (read-only)
      tmp/uploads/<id>       uploads in progress (expire after 24 h without activity)
      derived/ab/cd/<sha256>/thumb-v<N>.webp   thumbnails, per distinct content (plus preview-v<N>.webp for PDFs)
      cache/previews/ab/<sha256>-<bucket>-r<N>.{jpg,webp}   photo previews rendered on request (a bounded cache)
      geo/                   the GeoNames dataset (scripts/install-geodata.sh)

The database holds names, folders and who owns what; the blobs hold the bytes. Back up
both together (Phase 7 documents `pg_dump` + blob rsync). Files deleted from the trash
(or after 30 days in it) are removed from `blobs/` a day later, once nothing references them.
Uploads are refused while less than 1 GiB would remain free on that disk.

The data directory is bound to its database: each database gets an installation id
(created by its migrations), written to `.squirrel-instance` the first time the server
starts on an empty data directory. The server refuses to start when the marker names
another installation, or when the directory holds files but no marker, because the
maintenance job would otherwise treat every file the database does not know as an orphan
and delete it. Restore the database and the data directory as a pair.

A data directory from before markers existed (or restored without its marker) is adopted
once, deliberately, with the database it belongs to:

    squirrel migrate                         # SQUIRREL_DATABASE_URL set as for serve
    squirrel admin adopt-data-dir            # SQUIRREL_DATABASE_URL and SQUIRREL_DATA_DIR set

It prints how many stored files the database knows. It refuses a directory bound to another
installation, and one whose files the database knows none of (pass `--force` if you are sure).
As a second safety net, the orphan sweep skips deleting when the catalogue is empty or more
than 20 files making up over 10% of those examined look orphaned, and it first runs 6 hours
after the server starts.

## Administration

    SQUIRREL_DATABASE_URL=… cargo run -p squirrel -- admin set-quota alice 10737418240   # 10 GiB
    SQUIRREL_DATABASE_URL=… cargo run -p squirrel -- admin set-quota alice unlimited

`admin adopt-data-dir` is described under "Data directory" above.

Quotas are in bytes and count live files (not the trash). Users see their usage on the Files page.
`squirrel admin` never migrates: it refuses to run while migrations are pending
(run `squirrel migrate`, or start the server, first).

**Postgres** needs the pgvector extension (`deploy/docker-compose.dev.yml` pins the
`pgvector/pgvector:0.8.7-pg17` image by digest). pgvector is not a trusted extension: the
role that migrates must be a superuser (the compose role is), or a superuser must run
`CREATE EXTENSION vector;` in the database first. The server refuses to migrate otherwise.

## Media processing

Thumbnails, photo metadata (date, camera, place) and document text are computed in the
background by a job worker, from a queue kept in Postgres. Run it as its own process (the
default for production, so a decoder failure can never take the web server down):

    squirrel worker                       # same SQUIRREL_DATABASE_URL and SQUIRREL_DATA_DIR as serve

or inside the server for development and small installs: `squirrel serve --worker`
(`SQUIRREL_EMBEDDED_WORKER=true`). The worker never migrates: it waits for the server (or
`squirrel migrate`) to apply its migrations. Files stored before this existed are processed
by a backfill job queued at every worker start.

| Variable | Default | Purpose |
| --- | --- | --- |
| `SQUIRREL_WORKER_CONCURRENCY` | half the cores, at most 2 | Jobs run at once |
| `SQUIRREL_WORKER_KINDS` | every kind | Comma-separated job kinds this worker runs, e.g. `clip_embed,faces,cluster_faces` on a separate ML machine. Unknown names, or a list that leaves nothing runnable, stop the worker. Include `noop` for `squirrel admin jobs ping` to reach this worker |
| `SQUIRREL_WORKER_EXCLUDE_KINDS` | none | Comma-separated job kinds this worker leaves to others. Not with `SQUIRREL_WORKER_KINDS` |
| `SQUIRREL_ML_CONCURRENCY` | `1` | Background ML jobs (`clip_embed`, `faces`, `label_bank`, `cluster_faces`) run at once, so another slot stays free for fresh uploads. Only `1` is accepted for now: one `ml-exec` child serves one request at a time |
| `SQUIRREL_CLUSTER_DELAY_SECS` | `600` | How long new faces wait before they are grouped into people (at most one clustering run per user and delay while photos keep arriving) |
| `SQUIRREL_DUPLICATES_DELAY_SECS` | `60` | How long new photos wait before they are grouped with their near-identical copies (at most one grouping run per user and delay while photos keep arriving) |
| `SQUIRREL_MODELS_DIR` | `<data_dir>/models` | Pinned ML models (`scripts/fetch-models.sh`). Checked against their SHA-256 at start: a mismatch stops the worker. Without models, photo tags, search and faces are off |
| `SQUIRREL_ORT_DYLIB` | unset | The pinned ONNX Runtime library (`scripts/fetch-onnxruntime.sh`), required once models are installed |
| `SQUIRREL_ML_THREADS` | half the cores | Threads for one inference |
| `SQUIRREL_ML_MAX_MEMORY_MB` | `6144` | Address-space limit of each inference child (Linux). It grows with the thread count: raise it for many threads |
| `SQUIRREL_ML_EXEC` | `subprocess` | `subprocess` runs inference in a long-lived `squirrel ml-exec` child (resource limits, restarted on crash, stopped after 10 idle minutes; the server's search model after 5); `in-process` is for tests only: it loads the models at start and keeps them loaded |
| `SQUIRREL_MEDIA_EXEC` | `subprocess` | `subprocess` decodes untrusted files in short-lived `squirrel media-exec` children with memory, CPU and file limits; `in-process` is for tests only |
| `SQUIRREL_PDFIUM_DIR` | unset | Directory holding the pdfium library. Without it, PDFs get no thumbnail or text |
| `SQUIRREL_GEODATA_FILE` | `<data_dir>/geo/…` | Another place-name file (tests, custom extracts); used without the SHA-256 pin |

**Models** (photo tags, text search and faces) are fetched once, from revision-pinned URLs,
and checked against their SHA-256 (about 1.5 GB; sources and licences in
`crates/squirrel-ml/MODELS.md`):

    scripts/fetch-models.sh               # prints SQUIRREL_MODELS_DIR=…/.cache/squirrel-assets/models

They run on **ONNX Runtime** 1.31.1 (CPU, MIT), loaded at run time. Microsoft's official
NuGet package is downloaded (cached in `.cache/squirrel-assets/onnxruntime-cache/`) and checked against its
pinned SHA-256 and SHA-512; only this platform's library, `LICENSE` and
`ThirdPartyNotices.txt` are unpacked into `.cache/squirrel-assets/onnxruntime/`. The library's own SHA-256 is
checked again before every load:

    scripts/fetch-onnxruntime.sh          # prints SQUIRREL_ORT_DYLIB=…/.cache/squirrel-assets/onnxruntime/lib/…

In production, make the models directory and the ONNX Runtime library **read-only to the
service user** (owned by root, mode 0644/0755): they are checked when loaded, but a writable
library could be swapped between its check and `dlopen`. Models are hashed in memory and
loaded from that same buffer, which briefly doubles the child's memory while it starts
(see "Memory" under "Machine learning").

**pdfium** (PDF thumbnails and text) is downloaded, checked against its pinned SHA-256 and
unpacked into `target/pdfium/` by:

    scripts/fetch-pdfium.sh               # prints SQUIRREL_PDFIUM_DIR=…/target/pdfium/lib

It is Google's PDF engine (BSD-3-Clause / Apache-2.0); its `LICENSE` and the third-party
notices in `licenses/` stay next to the library and ship with it.

**Places** come from an offline GeoNames snapshot committed in `data/geodata/` (no request
leaves the server). Install it into a data directory with `scripts/install-geodata.sh <data_dir>`;
until then photos keep their coordinates and get their place once it is installed. Place names:
GeoNames (CC BY 4.0), credited in the photo viewer. `data/geodata/README.md` explains how to
rebuild the snapshot (`scripts/build-geodata.sh <out_dir>`, then update the pin).

**HEIC/HEIF** photos are decoded only in builds with `--features heic`, which need the system
libheif ≥ 1.17 with its libde265 HEVC plugin (`apt install libheif-dev libheif-plugin-libde265`,
`brew install libheif`). Other builds still read their date and place, and show them with
an "unsupported preview" tile.

**Previews** (the lightbox) are rendered on request from the original, at the screen's size:
clients send `px` (device pixels) and get the 1440, 2160 or 3200 px bucket above it (never
larger than the photo), kept in a least-recently-used cache (`SQUIRREL_PREVIEW_CACHE_MB`, not
counted in quotas). Thumbnails, and PDF previews, stay stored. Each render runs in its own
`squirrel media-exec` child (1.5 GiB address space on Linux, 10 s of CPU), through **libvips**
in builds with `--features vips` (the Docker image): shrink-on-load JPEG ≈ 0.1–0.3 s, HEIC
≈ 0.5 s for 12 MP. It needs the system libvips ≥ 8.15 (`apt install --no-install-recommends
libvips-dev`, `brew install vips`; LGPL-2.1+, linked dynamically) and only lets it load JPEG, PNG,
WebP, TIFF, GIF and HEIF. Without it, or when `squirrel media-exec --probe` finds it broken or
missing a loader, the built-in decoders render previews 2–3× slower. Per user, 40 renders then 4 a
second (429 beyond); cached previews cost nothing.

    squirrel admin previews stats         # SQUIRREL_DATA_DIR set: entries, size, renderer
    squirrel admin previews clear         # rendered again on request

Installations from before on-demand previews still hold a stored 1440 px preview per photo
(≈ 7–15 GB per 50 000 photos). Once the new release serves previews, delete them once with
`squirrel admin media drop-previews` (a worker deletes them 500 photos at a time; PDFs keep theirs).

**Duplicates** are near-identical photos of one user: different files that look the same
(resized, re-compressed, lightly edited; byte-identical files are already stored once). Each
photo gets a 64-bit perceptual hash of its thumbnail (DCT pHash); two photos are duplicates when
their hashes differ by at most 6 bits and they were taken at most 24 hours apart. A group is
anchored at its oldest photo and only photos close to that anchor join it, so a day of bursts
never chains into one group. Groups never mix users and need no models. In Explore › Duplicates (a
grid of groups; a group opens in the photo viewer with its photos beside it) the user keeps
one photo of a group: the others are hidden from every photo view (timeline,
search, albums, people, places, Explore, the phone) but stay in Files, count in the quota and
can be shown again from the Resolved tab. Nothing is hidden without that choice; a new copy that
joins a resolved group reopens it. Trashing a photo takes it out of its group (and unhides it).

**Document search** looks at file names, PDFs and text files: words of three letters or more
match as prefixes, two-letter words only as whole words, one-letter words are ignored. Each
search runs at most 5 s (then answers 503) and is rate limited per client (20 at once, then one
per second).

## Machine learning

With the models installed, the worker tags photos ("beach", "dog", "snow"), makes them
searchable by what they show, in any language ("dog on the beach", "chien sur la plage"),
and finds faces and groups
them into people each user can name, merge, split and hide. Everything runs on this
server's CPU, below thumbnails in priority, and nothing leaves the server. Without models
these features are simply off (Media status says so).

| What | Model | Size | Licence |
| --- | --- | --- | --- |
| Tags and search | Google SigLIP2 ViT-B/16, 224 px, multilingual (ONNX export `immich-app/ViT-B-16-SigLIP2__webli`, 768-d): image tower | 354 MB | Apache-2.0 |
| Search queries and the tag bank | the same SigLIP2's text tower (Gemma tokenizer), run by the server on the first search | 1 077 MB | Apache-2.0 |
| Face detection | OpenCV Zoo YuNet (`face_detection_yunet_2026may`) | 0.2 MB | MIT |
| Face recognition | OpenCV Zoo SFace (`face_recognition_sface_2021dec`, 128-d) | 37 MB | Apache-2.0 |
| Runtime | ONNX Runtime 1.31.1, CPU (Microsoft's NuGet package) | ≈ 20 MB | MIT |

Every file is fetched from a revision-pinned URL by `scripts/fetch-models.sh` and
`scripts/fetch-onnxruntime.sh`, which check its SHA-256 (the hashes live in those scripts and
in `crates/squirrel-ml/src/models.rs`, a test keeps them equal) and keep each upstream licence
next to the files. `fetch-models.sh --without-faces` and `--clip-only-image` (tags without
text search) fetch less for small boxes. **InsightFace models (SCRFD, ArcFace, `buffalo_l`)
are never used**: their licence allows non-commercial research only, which does not cover a
home server; YuNet and SFace are permissively licensed, at the cost of somewhat more split
people (merging is one click).

**Running ML on another machine.** Inference reads originals (it renders a temporary 1440 px
frame in the system's temp directory) and writes Postgres, so a bigger machine can share the
data directory read-only:
`squirrel worker --kinds clip_embed,faces,cluster_faces,label_bank` there, and
`squirrel worker --exclude-kinds clip_embed,faces,cluster_faces,label_bank` on the home
server (`SQUIRREL_WORKER_KINDS` / `SQUIRREL_WORKER_EXCLUDE_KINDS`). `--ml-concurrency` stays 1.

**Memory.** Inference runs in long-lived `squirrel ml-exec` children, restarted on a crash and
stopped when idle, with an address-space limit of 6 GiB each on Linux
(`SQUIRREL_ML_MAX_MEMORY_MB`). Measured on Linux
(arm64, release, SigLIP2): the worker's child (image embedding and faces) takes about 0.5–0.6 GiB
resident. The text model takes about 1.9 GiB resident; it runs in the server while searches
happen (stopped after 5 idle minutes) and in the worker while it builds the label bank (once per
model and label list). The reserved address space (VmPeak) is at most 2.1 GiB at 1–4 threads,
and 3.9 GiB with every model in one child at 64 threads, the most allowed. The children start
with `MALLOC_ARENA_MAX=2`: without it glibc reserves 64 MB per thread, and 32 threads need far
more than the limit. Plan for 4 GB of RAM on the box.

**Time per photo** (measured on an Apple M4 Pro, release build, 2 threads): the SigLIP2 image
embedding takes about 48 ms (43 ms in the model) and face detection with embedding about 21 ms,
on top of the thumbnails; a search query takes about 15 ms once the text model is loaded
(≈ 4 s to verify and load it). An Intel i3-1220P mini PC (NUC 12 Pro) should take roughly
0.12–0.16 s per photo for the embedding and 0.2 s with faces (an estimate, not measured:
about 3× slower per inference), so ≈ 18 000 photos an hour; a query about 40–50 ms. The work
runs in the background, below thumbnails, one photo at a time.

**Postgres** needs pgvector (see "Administration" above): embeddings are `halfvec` columns,
searched exactly per user (no shared index), so one user's library never changes another's
results.

**Faces are on by default** and stay with their owner: a photo shared by two users gets
separate faces and people for each. Settings › Faces turns them off, which at once deletes
all of that user's faces, people, names and corrections (turning them on again finds the faces
anew, without names). Deleting a photo for good deletes its faces too.

**How tags and search decide.** SigLIP2 scores every photo–text pair with a probability,
`σ(112.67 · cosine − 16.77)` (its learned scale and bias). A tag needs a probability of
0.0025 (logit −6), a search result 0.0005 (logit −7.6): SigLIP gives a bare word such as
"dog" a low absolute probability even on a clear match (logit ≈ −3.6), while unrelated pairs
score −8.7 or less on the test photos. Labels easily mistaken for each other share a group in
the label bank (`crates/squirrel-ml/labels/labels-v3.tsv`: dog, cat, rabbit…; car, truck,
bus…): only the best of a group can become a tag, and only one logit ahead of its siblings.
A query close to such a label ("dog", "chien", "perro") must also beat that label's siblings
on each photo, so a search for "dog" leaves out a photo where "cat" scores higher. These
values were calibrated on the test photos (public-domain dogs, cats and portraits, and
generated shapes) and await a check on a real library, like the face detector score (0.6)
and the clustering parameters (`squirrel_ml::cluster::SFACE`: distance 0.50, 3 faces,
detector score 0.7 to found a person). To judge tags or searches on your own photos:

    SQUIRREL_ORT_DYLIB=… SQUIRREL_MODELS_DIR=… cargo run --release -p squirrel-ml \
      --features test-util --example tag_report -- <a folder of photos>
    … --example search_report -- <a folder of photos> "chien" "red car"

After changing the clustering parameters, regroup a user's automatic faces (their own
corrections, names and hidden people stay):

    squirrel admin faces recluster <username>

Administration:

    squirrel admin jobs                   # queue counts per kind, and the latest failures
    squirrel admin jobs retry <id>        # run a failed job again
    squirrel admin media backfill         # queue processing for anything not processed yet
