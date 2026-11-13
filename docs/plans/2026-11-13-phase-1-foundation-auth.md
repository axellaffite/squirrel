# Phase 1 — Foundation, Auth & Invitations Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** A running Squirrel server where the first visitor creates the admin account, any logged-in user can create single-use invitation links, invited people can join, and everyone can log in and out through a Svelte web UI.

**Architecture:** Cargo workspace with small crates (`squirrel-core` pure domain types, `squirrel-auth` crypto primitives, `squirrel-db` sqlx repositories + migrations, `squirrel-api` axum routes + OpenAPI, `squirrel` binary). Postgres 17 (pgvector image, ready for Phase 5). Server-side sessions in an `HttpOnly` cookie plus a synchronizer CSRF token. The Svelte SPA is built statically and embedded in the binary; its typed API client is generated from the committed `openapi.json`.

**Tech Stack:** Rust 1.94 (edition 2024), axum 0.8, sqlx 0.8 (Postgres), argon2 0.5, utoipa 5 + utoipa-axum 0.2, tower_governor 0.7, rust-embed 8, axum-test 17; Svelte 5 + SvelteKit (adapter-static), openapi-typescript + openapi-fetch, Vitest, Playwright; Docker Compose for dev Postgres.

**Design reference:** `docs/plans/2026-11-13-squirrel-home-drive-design.md`

**Deliberate deviations from the design for Phase 1:**
- sqlx *runtime* queries (`query_as::<_, T>`) instead of compile-time macros, so the workspace builds without a live DB or `sqlx-cli`. Revisit once query count grows.
- `devices` table and mobile tokens are deferred to Phase 6 (mobile app).
- Cookie `Max-Age` is 365 days; the **server-side** `expires_at` (30-day sliding) is authoritative.

---

## Prerequisites (do once, before Task 1)

1. **Node 22 LTS** — current Vite requires Node ≥ 20.19 and the machine has 20.12.
   Install it with nvm, without changing the default Node: `nvm install 22`; `web/.nvmrc` pins it, so run `nvm use` in `web/` (or prefix commands with `source ~/.nvm/nvm.sh && nvm use 22 &&`).
   Verify: `node --version` → `v22.x`
2. **Docker** running (`docker info` succeeds).
3. **pnpm** ≥ 10 (`pnpm --version`).

Conventions for every task:
- Run commands from the repo root unless stated.
- Commit messages follow `<type>: <description>` (feat, fix, test, chore, docs, ci).
- After each Rust task: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings` must be clean before committing.

---

### Task 1: Workspace scaffold and dev Postgres

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `.cargo/config.toml`, `.gitignore`, `deploy/docker-compose.dev.yml`
- Create: `crates/squirrel-core/Cargo.toml`, `crates/squirrel-core/src/lib.rs`

**Step 1: Create the workspace manifest**

`Cargo.toml`:
```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.94"

[workspace.dependencies]
anyhow = "1"
argon2 = { version = "0.5", features = ["std"] }
axum = { version = "0.8", features = ["macros"] }
axum-extra = { version = "0.10", features = ["cookie"] }
axum-test = "17"
base64 = "0.22"
clap = { version = "4", features = ["derive", "env"] }
rand = "0.8"
rust-embed = { version = "8", features = ["mime-guess"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "postgres", "uuid", "time", "migrate", "macros"] }
subtle = "2"
thiserror = "2"
time = { version = "0.3", features = ["serde", "serde-well-known"] }
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace"] }
tower_governor = "0.7"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
utoipa = { version = "5", features = ["uuid", "time"] }
utoipa-axum = "0.2"
uuid = { version = "1", features = ["v7", "serde"] }

squirrel-core = { path = "crates/squirrel-core" }
squirrel-auth = { path = "crates/squirrel-auth" }
squirrel-db = { path = "crates/squirrel-db" }
squirrel-api = { path = "crates/squirrel-api" }
```

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "1.94"
components = ["rustfmt", "clippy"]
```

`.cargo/config.toml` (used by `sqlx::test` at test runtime; does not override an already-set env var):
```toml
[env]
DATABASE_URL = "postgres://squirrel:squirrel@localhost:55432/squirrel"
```

`.gitignore`:
```
/target
.env
web/node_modules
web/build
web/.svelte-kit
web/test-results
web/playwright-report
```

**Step 2: Dev Postgres**

`deploy/docker-compose.dev.yml`:
```yaml
services:
  postgres:
    image: pgvector/pgvector:pg17
    environment:
      POSTGRES_USER: squirrel
      POSTGRES_PASSWORD: squirrel
      POSTGRES_DB: squirrel
    ports:
      - "55432:5432"
    volumes:
      - squirrel-dev-pg:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U squirrel"]
      interval: 2s
      timeout: 3s
      retries: 20
volumes:
  squirrel-dev-pg:
```

**Step 3: Empty core crate so the workspace compiles**

`crates/squirrel-core/Cargo.toml`:
```toml
[package]
name = "squirrel-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
serde.workspace = true
thiserror.workspace = true
utoipa.workspace = true
```

`crates/squirrel-core/src/lib.rs`:
```rust
//! Pure domain types for Squirrel. No I/O lives here.
```

**Step 4: Verify**

Run: `cargo build --workspace && docker compose -f deploy/docker-compose.dev.yml up -d --wait`
Expected: build succeeds; `postgres` container reports `Healthy`.

**Step 5: Commit**
```bash
git add Cargo.toml rust-toolchain.toml .cargo .gitignore deploy crates/squirrel-core
git commit -m "chore: scaffold cargo workspace and dev postgres"
```

---

### Task 2: `Username` domain type

**Files:**
- Create: `crates/squirrel-core/src/username.rs`
- Modify: `crates/squirrel-core/src/lib.rs`

**Step 1: Write the failing tests** (bottom of `username.rs`; create the file with only the test module + `use super::*;` first)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_and_whitespace() {
        assert_eq!(Username::parse("  Alice ").unwrap().as_str(), "alice");
    }

    #[test]
    fn accepts_dots_underscores_dashes() {
        assert!(Username::parse("a.b_c-d").is_ok());
    }

    #[test]
    fn rejects_too_short_and_too_long() {
        assert_eq!(Username::parse("ab"), Err(UsernameError::Length));
        assert_eq!(Username::parse(&"a".repeat(33)), Err(UsernameError::Length));
    }

    #[test]
    fn rejects_invalid_characters() {
        assert_eq!(Username::parse("al ice"), Err(UsernameError::Charset));
        assert_eq!(Username::parse("alïce"), Err(UsernameError::Charset));
    }

    #[test]
    fn must_start_with_letter_or_digit() {
        assert_eq!(Username::parse(".alice"), Err(UsernameError::Start));
    }
}
```

**Step 2: Run to verify failure**

Run: `cargo test -p squirrel-core`
Expected: FAIL — `cannot find type Username`.

**Step 3: Implement** (top of `username.rs`)

```rust
/// A validated, lowercase username: 3–32 chars of `[a-z0-9._-]`, starting with `[a-z0-9]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Username(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UsernameError {
    #[error("username must be between 3 and 32 characters")]
    Length,
    #[error("username may only contain lowercase letters, digits, '.', '_' and '-'")]
    Charset,
    #[error("username must start with a letter or a digit")]
    Start,
}

impl Username {
    pub const MIN_LEN: usize = 3;
    pub const MAX_LEN: usize = 32;

    pub fn parse(raw: &str) -> Result<Self, UsernameError> {
        let normalized = raw.trim().to_lowercase();
        let len = normalized.chars().count();
        if !(Self::MIN_LEN..=Self::MAX_LEN).contains(&len) {
            return Err(UsernameError::Length);
        }
        let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-');
        if !normalized.chars().all(allowed) {
            return Err(UsernameError::Charset);
        }
        if !normalized.starts_with(|c: char| c.is_ascii_alphanumeric()) {
            return Err(UsernameError::Start);
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

`lib.rs`:
```rust
//! Pure domain types for Squirrel. No I/O lives here.

mod username;

pub use username::{Username, UsernameError};
```

**Step 4: Run tests** — `cargo test -p squirrel-core` → PASS (5 tests).

**Step 5: Commit** — `git commit -am "feat(core): add validated Username type"` (add new file first: `git add crates/squirrel-core`).

---

### Task 3: `Password`, `DisplayName`, `Role`

**Files:**
- Create: `crates/squirrel-core/src/password.rs`, `display_name.rs`, `role.rs`
- Modify: `crates/squirrel-core/src/lib.rs`

**Step 1: Failing tests**

`password.rs` tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_length_bounds() {
        assert_eq!(Password::parse("short").unwrap_err(), PasswordError::TooShort);
        assert_eq!(Password::parse(&"x".repeat(257)).unwrap_err(), PasswordError::TooLong);
        assert!(Password::parse("long enough!").is_ok());
    }

    #[test]
    fn debug_never_prints_the_secret() {
        let p = Password::parse("super secret value").unwrap();
        assert!(!format!("{p:?}").contains("secret"));
    }
}
```

`display_name.rs` tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_bounds_length() {
        assert_eq!(DisplayName::parse("  Alice  ").unwrap().as_str(), "Alice");
        assert_eq!(DisplayName::parse("   "), Err(DisplayNameError::Length));
        assert_eq!(DisplayName::parse(&"é".repeat(65)), Err(DisplayNameError::Length));
    }

    #[test]
    fn rejects_control_characters() {
        assert_eq!(DisplayName::parse("Al\u{0}ice"), Err(DisplayNameError::Control));
    }
}
```

`role.rs` tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_str() {
        for role in [Role::Admin, Role::User] {
            assert_eq!(role.as_str().parse::<Role>().unwrap(), role);
        }
        assert!("root".parse::<Role>().is_err());
    }
}
```

**Step 2:** `cargo test -p squirrel-core` → FAIL (types missing).

**Step 3: Implement**

`password.rs`:
```rust
use std::fmt;

/// A candidate password that passed length policy. Never printed by `Debug`.
#[derive(Clone)]
pub struct Password(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PasswordError {
    #[error("password must be at least {} characters", Password::MIN_LEN)]
    TooShort,
    #[error("password must be at most {} characters", Password::MAX_LEN)]
    TooLong,
}

impl Password {
    pub const MIN_LEN: usize = 10;
    /// Upper bound protects the server from hashing huge inputs.
    pub const MAX_LEN: usize = 256;

    pub fn parse(raw: &str) -> Result<Self, PasswordError> {
        let len = raw.chars().count();
        if len < Self::MIN_LEN {
            return Err(PasswordError::TooShort);
        }
        if len > Self::MAX_LEN {
            return Err(PasswordError::TooLong);
        }
        Ok(Self(raw.to_owned()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(<redacted>)")
    }
}
```

`display_name.rs`:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayName(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DisplayNameError {
    #[error("display name must be between 1 and 64 characters")]
    Length,
    #[error("display name must not contain control characters")]
    Control,
}

impl DisplayName {
    pub const MAX_LEN: usize = 64;

    pub fn parse(raw: &str) -> Result<Self, DisplayNameError> {
        let trimmed = raw.trim();
        let len = trimmed.chars().count();
        if len == 0 || len > Self::MAX_LEN {
            return Err(DisplayNameError::Length);
        }
        if trimmed.chars().any(char::is_control) {
            return Err(DisplayNameError::Control);
        }
        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

`role.rs`:
```rust
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
}

#[derive(Debug, thiserror::Error)]
#[error("unknown role: {0}")]
pub struct UnknownRole(pub String);

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::User => "user",
        }
    }

    pub fn is_admin(self) -> bool {
        self == Role::Admin
    }
}

impl FromStr for Role {
    type Err = UnknownRole;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin" => Ok(Role::Admin),
            "user" => Ok(Role::User),
            other => Err(UnknownRole(other.to_owned())),
        }
    }
}
```

`lib.rs`:
```rust
//! Pure domain types for Squirrel. No I/O lives here.

mod display_name;
mod password;
mod role;
mod username;

pub use display_name::{DisplayName, DisplayNameError};
pub use password::{Password, PasswordError};
pub use role::{Role, UnknownRole};
pub use username::{Username, UsernameError};
```

**Step 4:** `cargo test -p squirrel-core` → PASS.

**Step 5: Commit** — `git add crates/squirrel-core && git commit -m "feat(core): add Password, DisplayName and Role types"`

---

### Task 4: Password hashing (`squirrel-auth`)

**Files:**
- Create: `crates/squirrel-auth/Cargo.toml`, `src/lib.rs`, `src/password.rs`

**Step 1: Crate manifest**
```toml
[package]
name = "squirrel-auth"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
argon2.workspace = true
base64.workspace = true
rand.workspace = true
sha2.workspace = true
squirrel-core.workspace = true
thiserror.workspace = true
```

**Step 2: Failing tests** (`password.rs`)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use squirrel_core::Password;

    fn pw(s: &str) -> Password {
        Password::parse(s).unwrap()
    }

    #[test]
    fn uses_argon2id_with_pinned_parameters() {
        let hash = hash_password(&pw("correct horse battery")).unwrap();
        assert!(hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"), "{hash}");
    }

    #[test]
    fn verifies_correct_and_rejects_wrong_password() {
        let hash = hash_password(&pw("correct horse battery")).unwrap();
        assert!(verify_password("correct horse battery", &hash).unwrap());
        assert!(!verify_password("wrong horse battery", &hash).unwrap());
    }

    #[test]
    fn same_password_gets_different_salts() {
        let a = hash_password(&pw("correct horse battery")).unwrap();
        let b = hash_password(&pw("correct horse battery")).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn malformed_hash_is_an_error() {
        assert!(matches!(verify_password("x", "not-a-hash"), Err(HashError::Malformed)));
    }
}
```

**Step 3:** `cargo test -p squirrel-auth` → FAIL.

**Step 4: Implement** (`password.rs`)
```rust
use std::sync::OnceLock;

use argon2::password_hash::{self, SaltString, rand_core::OsRng};
use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use squirrel_core::Password;

/// OWASP-recommended Argon2id parameters (19 MiB, 2 iterations, 1 lane).
const MEMORY_KIB: u32 = 19 * 1024;
const ITERATIONS: u32 = 2;
const PARALLELISM: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum HashError {
    #[error("password hashing failed: {0}")]
    Hash(password_hash::Error),
    #[error("stored password hash is malformed")]
    Malformed,
}

fn argon2() -> Argon2<'static> {
    let params = Params::new(MEMORY_KIB, ITERATIONS, PARALLELISM, None)
        .expect("static Argon2 parameters are valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// CPU-heavy: call from `spawn_blocking`.
pub fn hash_password(password: &Password) -> Result<String, HashError> {
    let salt = SaltString::generate(&mut OsRng);
    argon2()
        .hash_password(password.expose().as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(HashError::Hash)
}

/// CPU-heavy: call from `spawn_blocking`.
pub fn verify_password(candidate: &str, stored_hash: &str) -> Result<bool, HashError> {
    let parsed = PasswordHash::new(stored_hash).map_err(|_| HashError::Malformed)?;
    match argon2().verify_password(candidate.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(password_hash::Error::Password) => Ok(false),
        Err(other) => Err(HashError::Hash(other)),
    }
}

/// Spend the same time as a real verification, so unknown usernames are not
/// distinguishable from wrong passwords by timing. CPU-heavy.
pub fn burn_verification_time(candidate: &str) {
    static DUMMY_HASH: OnceLock<String> = OnceLock::new();
    let hash = DUMMY_HASH.get_or_init(|| {
        let dummy = Password::parse("timing-equalizer-password").expect("valid dummy password");
        hash_password(&dummy).expect("hashing the dummy password succeeds")
    });
    let _ = verify_password(candidate, hash);
}
```

`lib.rs`:
```rust
//! Cryptographic primitives: password hashing and opaque tokens.

mod password;

pub use password::{HashError, burn_verification_time, hash_password, verify_password};
```

**Step 5:** `cargo test -p squirrel-auth` → PASS (tests take ~1 s because Argon2 is deliberately slow).

**Step 6: Commit** — `git add crates/squirrel-auth && git commit -m "feat(auth): add Argon2id password hashing"`

---

### Task 5: Opaque tokens

**Files:**
- Create: `crates/squirrel-auth/src/token.rs`
- Modify: `crates/squirrel-auth/src/lib.rs`

**Step 1: Failing tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_43_url_safe_chars_and_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 43);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(a, b);
    }

    #[test]
    fn hash_is_deterministic_sha256() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_eq!(hash_token("abc").len(), 32);
        assert_ne!(hash_token("abc"), hash_token("abd"));
    }
}
```

**Step 2:** `cargo test -p squirrel-auth token` → FAIL.

**Step 3: Implement**
```rust
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

const TOKEN_BYTES: usize = 32;

/// 256-bit random token, base64url without padding (43 chars).
pub fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Tokens are stored only as their SHA-256 digest.
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}
```
Add to `lib.rs`: `mod token;` and `pub use token::{generate_token, hash_token};`

**Step 4:** `cargo test -p squirrel-auth` → PASS.

**Step 5: Commit** — `git add crates/squirrel-auth && git commit -m "feat(auth): add opaque token generation and hashing"`

---

### Task 6: Database crate, migration, pool

**Files:**
- Create: `crates/squirrel-db/Cargo.toml`, `src/lib.rs`, `src/error.rs`, `migrations/20261113000001_initial.sql`
- Test: `crates/squirrel-db/tests/migrations.rs`

**Step 1: Manifest**
```toml
[package]
name = "squirrel-db"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
squirrel-core.workspace = true
sqlx.workspace = true
thiserror.workspace = true
time.workspace = true
uuid.workspace = true

[dev-dependencies]
tokio.workspace = true
```

**Step 2: Failing test** (`tests/migrations.rs`)
```rust
use sqlx::PgPool;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn creates_singleton_instance_settings(pool: PgPool) {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM instance_settings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
```

**Step 3:** `cargo test -p squirrel-db` → FAIL (`MIGRATOR` missing).

**Step 4: Migration** (`migrations/20261113000001_initial.sql`)
```sql
CREATE TABLE users (
    id             UUID PRIMARY KEY,
    username       TEXT NOT NULL UNIQUE,
    display_name   TEXT NOT NULL,
    password_hash  TEXT NOT NULL,
    role           TEXT NOT NULL CHECK (role IN ('admin', 'user')),
    quota_bytes    BIGINT CHECK (quota_bytes IS NULL OR quota_bytes >= 0),
    invited_by     UUID REFERENCES users (id) ON DELETE SET NULL,
    failed_logins  INTEGER NOT NULL DEFAULT 0,
    locked_until   TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    disabled_at    TIMESTAMPTZ
);

CREATE TABLE invitations (
    id          UUID PRIMARY KEY,
    token_hash  BYTEA NOT NULL UNIQUE,
    created_by  UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    email_hint  TEXT,
    max_uses    INTEGER NOT NULL DEFAULT 1 CHECK (max_uses > 0),
    used_count  INTEGER NOT NULL DEFAULT 0 CHECK (used_count >= 0),
    expires_at  TIMESTAMPTZ NOT NULL,
    revoked_at  TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX invitations_created_by_idx ON invitations (created_by, created_at DESC);

CREATE TABLE sessions (
    id            UUID PRIMARY KEY,
    token_hash    BYTEA NOT NULL UNIQUE,
    user_id       UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    csrf_token    TEXT NOT NULL,
    user_agent    TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_user_idx ON sessions (user_id);

CREATE TABLE instance_settings (
    id                  BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    invites_admin_only  BOOLEAN NOT NULL DEFAULT FALSE
);
INSERT INTO instance_settings DEFAULT VALUES;
```

`src/error.rs`:
```rust
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("an account already exists")]
    NotFirstUser,
    #[error("username is already taken")]
    UsernameTaken,
    #[error("invitation is invalid, expired or already used")]
    InvitationInvalid,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub(crate) fn is_unique_violation(err: &sqlx::Error, constraint: &str) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation() && db.constraint() == Some(constraint))
}
```

`src/lib.rs`:
```rust
//! Postgres repositories and migrations.

mod error;

pub use error::DbError;

use sqlx::PgPool;
use sqlx::migrate::{MigrateError, Migrator};
use sqlx::postgres::PgPoolOptions;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

const MAX_CONNECTIONS: u32 = 16;

pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .connect(database_url)
        .await
}

pub async fn migrate(pool: &PgPool) -> Result<(), MigrateError> {
    MIGRATOR.run(pool).await
}
```

**Step 5:** Postgres must be up (`docker compose -f deploy/docker-compose.dev.yml up -d --wait`). Run `cargo test -p squirrel-db` → PASS.

**Step 6: Commit** — `git add crates/squirrel-db && git commit -m "feat(db): add initial schema and migrator"`

---

### Task 7: Users repository with race-safe first-admin bootstrap

**Files:**
- Create: `crates/squirrel-db/src/users.rs`
- Modify: `crates/squirrel-db/src/lib.rs` (add `pub mod users;`)
- Test: `crates/squirrel-db/tests/users.rs`

**Step 1: Failing tests**
```rust
use sqlx::PgPool;
use squirrel_core::{DisplayName, Role, Username};
use squirrel_db::DbError;
use squirrel_db::users::{self, NewUser};
use tokio::task::JoinSet;

fn new_user(name: &str) -> NewUser {
    NewUser {
        username: Username::parse(name).unwrap(),
        display_name: DisplayName::parse(name).unwrap(),
        password_hash: "not-a-real-hash".into(),
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn first_user_becomes_admin(pool: PgPool) {
    assert!(!users::any_exists(&pool).await.unwrap());
    let user = users::create_first_admin(&pool, &new_user("alice")).await.unwrap();
    assert_eq!(user.role(), Role::Admin);
    assert!(users::any_exists(&pool).await.unwrap());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn second_bootstrap_is_rejected(pool: PgPool) {
    users::create_first_admin(&pool, &new_user("alice")).await.unwrap();
    let err = users::create_first_admin(&pool, &new_user("bob")).await.unwrap_err();
    assert!(matches!(err, DbError::NotFirstUser));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn concurrent_bootstrap_creates_exactly_one_admin(pool: PgPool) {
    let mut attempts = JoinSet::new();
    for i in 0..5 {
        let pool = pool.clone();
        attempts.spawn(async move { users::create_first_admin(&pool, &new_user(&format!("admin{i}"))).await });
    }
    let mut created = 0;
    while let Some(result) = attempts.join_next().await {
        match result.unwrap() {
            Ok(_) => created += 1,
            Err(DbError::NotFirstUser) => {}
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
    assert_eq!(created, 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn finds_users_by_username_and_id(pool: PgPool) {
    let alice = users::create_first_admin(&pool, &new_user("alice")).await.unwrap();
    let by_name = users::find_by_username(&pool, &Username::parse("ALICE").unwrap()).await.unwrap().unwrap();
    assert_eq!(by_name.id, alice.id);
    assert!(users::find_by_id(&pool, alice.id).await.unwrap().is_some());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn locks_after_five_failures_and_resets_on_success(pool: PgPool) {
    let alice = users::create_first_admin(&pool, &new_user("alice")).await.unwrap();
    for _ in 0..4 {
        users::record_login_failure(&pool, alice.id).await.unwrap();
    }
    let now = time::OffsetDateTime::now_utc();
    assert!(!users::find_by_id(&pool, alice.id).await.unwrap().unwrap().is_locked(now));

    users::record_login_failure(&pool, alice.id).await.unwrap();
    assert!(users::find_by_id(&pool, alice.id).await.unwrap().unwrap().is_locked(now));

    users::record_login_success(&pool, alice.id).await.unwrap();
    let reset = users::find_by_id(&pool, alice.id).await.unwrap().unwrap();
    assert_eq!(reset.failed_logins, 0);
    assert!(!reset.is_locked(now));
}
```
Add `time.workspace = true` to `[dev-dependencies]` of `squirrel-db`.

**Step 2:** `cargo test -p squirrel-db --test users` → FAIL.

**Step 3: Implement** (`src/users.rs`)
```rust
use sqlx::{FromRow, PgConnection, PgPool};
use squirrel_core::{DisplayName, Role, Username};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;
use crate::error::is_unique_violation;

/// Arbitrary constant identifying the "first user" advisory lock.
const BOOTSTRAP_LOCK_KEY: i64 = 0x5351_5552_0001;
/// Failed logins before the account is temporarily locked.
pub const LOCKOUT_THRESHOLD: i32 = 5;

const USER_COLUMNS: &str = "id, username, display_name, password_hash, role, quota_bytes, \
     invited_by, failed_logins, locked_until, created_at, disabled_at";

#[derive(Debug, Clone, FromRow)]
pub struct UserRecord {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    role: String,
    pub quota_bytes: Option<i64>,
    pub invited_by: Option<Uuid>,
    pub failed_logins: i32,
    pub locked_until: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub disabled_at: Option<OffsetDateTime>,
}

impl UserRecord {
    /// The DB CHECK constraint guarantees a known value; fall back to least privilege anyway.
    pub fn role(&self) -> Role {
        self.role.parse().unwrap_or(Role::User)
    }

    pub fn is_locked(&self, now: OffsetDateTime) -> bool {
        self.locked_until.is_some_and(|until| until > now)
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled_at.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub username: Username,
    pub display_name: DisplayName,
    pub password_hash: String,
}

pub async fn any_exists(pool: &PgPool) -> Result<bool, DbError> {
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)").fetch_one(pool).await?)
}

/// Creates the admin account only if no user exists yet. Serialized with an
/// advisory transaction lock, so concurrent callers cannot both succeed.
pub async fn create_first_admin(pool: &PgPool, new_user: &NewUser) -> Result<UserRecord, DbError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(BOOTSTRAP_LOCK_KEY)
        .execute(&mut *tx)
        .await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
        .fetch_one(&mut *tx)
        .await?;
    if exists {
        return Err(DbError::NotFirstUser);
    }
    let user = insert(&mut tx, new_user, Role::Admin, None).await?;
    tx.commit().await?;
    Ok(user)
}

/// Atomically consumes one use of the invitation and creates a regular user.
/// Any failure (taken username, invalid token) rolls the invitation use back.
pub async fn create_with_invitation(
    pool: &PgPool,
    new_user: &NewUser,
    token_hash: &[u8],
) -> Result<UserRecord, DbError> {
    let mut tx = pool.begin().await?;
    let inviter: Option<Uuid> = sqlx::query_scalar(
        "UPDATE invitations SET used_count = used_count + 1 \
         WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > now() AND used_count < max_uses \
         RETURNING created_by",
    )
    .bind(token_hash)
    .fetch_optional(&mut *tx)
    .await?;
    let inviter = inviter.ok_or(DbError::InvitationInvalid)?;
    let user = insert(&mut tx, new_user, Role::User, Some(inviter)).await?;
    tx.commit().await?;
    Ok(user)
}

async fn insert(
    conn: &mut PgConnection,
    new_user: &NewUser,
    role: Role,
    invited_by: Option<Uuid>,
) -> Result<UserRecord, DbError> {
    let sql = format!(
        "INSERT INTO users (id, username, display_name, password_hash, role, invited_by) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {USER_COLUMNS}"
    );
    sqlx::query_as::<_, UserRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(new_user.username.as_str())
        .bind(new_user.display_name.as_str())
        .bind(&new_user.password_hash)
        .bind(role.as_str())
        .bind(invited_by)
        .fetch_one(conn)
        .await
        .map_err(|err| {
            if is_unique_violation(&err, "users_username_key") {
                DbError::UsernameTaken
            } else {
                err.into()
            }
        })
}

pub async fn find_by_username(pool: &PgPool, username: &Username) -> Result<Option<UserRecord>, DbError> {
    let sql = format!("SELECT {USER_COLUMNS} FROM users WHERE username = $1");
    Ok(sqlx::query_as::<_, UserRecord>(&sql)
        .bind(username.as_str())
        .fetch_optional(pool)
        .await?)
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<UserRecord>, DbError> {
    let sql = format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1");
    Ok(sqlx::query_as::<_, UserRecord>(&sql).bind(id).fetch_optional(pool).await?)
}

/// Increments the failure counter; from the threshold on, locks the account for
/// 1, 2, 4… minutes, capped at one hour.
pub async fn record_login_failure(pool: &PgPool, id: Uuid) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE users SET \
           failed_logins = failed_logins + 1, \
           locked_until = CASE WHEN failed_logins + 1 >= $2 \
             THEN now() + LEAST(interval '1 minute' * power(2, failed_logins + 1 - $2), interval '1 hour') \
             ELSE locked_until END \
         WHERE id = $1",
    )
    .bind(id)
    .bind(LOCKOUT_THRESHOLD)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_login_success(pool: &PgPool, id: Uuid) -> Result<(), DbError> {
    sqlx::query("UPDATE users SET failed_logins = 0, locked_until = NULL WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
```

**Step 4:** `cargo test -p squirrel-db` → PASS.

**Step 5: Commit** — `git add crates/squirrel-db && git commit -m "feat(db): add users repository with race-safe bootstrap"`

---

### Task 8: Invitations repository

**Files:**
- Create: `crates/squirrel-db/src/invitations.rs`
- Modify: `crates/squirrel-db/src/lib.rs` (`pub mod invitations;`)
- Test: `crates/squirrel-db/tests/invitations.rs`

**Step 1: Failing tests**
```rust
use sqlx::PgPool;
use squirrel_core::{DisplayName, Role, Username};
use squirrel_db::DbError;
use squirrel_db::invitations::{self, InvitationStatus, NewInvitation};
use squirrel_db::users::{self, NewUser, UserRecord};
use time::{Duration, OffsetDateTime};

fn new_user(name: &str) -> NewUser {
    NewUser {
        username: Username::parse(name).unwrap(),
        display_name: DisplayName::parse(name).unwrap(),
        password_hash: "not-a-real-hash".into(),
    }
}

async fn admin(pool: &PgPool) -> UserRecord {
    users::create_first_admin(pool, &new_user("alice")).await.unwrap()
}

async fn invite(pool: &PgPool, by: &UserRecord, hash: &[u8], ttl: Duration) {
    invitations::create(
        pool,
        &NewInvitation { created_by: by.id, token_hash: hash, email_hint: None, expires_at: OffsetDateTime::now_utc() + ttl },
    )
    .await
    .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn redeeming_creates_regular_user_linked_to_inviter(pool: PgPool) {
    let alice = admin(&pool).await;
    invite(&pool, &alice, b"token-1", Duration::days(7)).await;

    let bob = users::create_with_invitation(&pool, &new_user("bob"), b"token-1").await.unwrap();
    assert_eq!(bob.role(), Role::User);
    assert_eq!(bob.invited_by, Some(alice.id));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invitation_is_single_use(pool: PgPool) {
    let alice = admin(&pool).await;
    invite(&pool, &alice, b"token-1", Duration::days(7)).await;
    users::create_with_invitation(&pool, &new_user("bob"), b"token-1").await.unwrap();

    let err = users::create_with_invitation(&pool, &new_user("carol"), b"token-1").await.unwrap_err();
    assert!(matches!(err, DbError::InvitationInvalid));
    assert!(!invitations::is_redeemable(&pool, b"token-1").await.unwrap());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn expired_and_revoked_invitations_are_rejected(pool: PgPool) {
    let alice = admin(&pool).await;
    invite(&pool, &alice, b"expired", Duration::seconds(-1)).await;
    invite(&pool, &alice, b"revoked", Duration::days(7)).await;
    let revoked_id = invitations::list_created_by(&pool, alice.id).await.unwrap()[0].id;
    assert!(invitations::revoke(&pool, revoked_id, alice.id).await.unwrap());

    for token in [b"expired".as_slice(), b"revoked".as_slice(), b"unknown".as_slice()] {
        let err = users::create_with_invitation(&pool, &new_user("bob"), token).await.unwrap_err();
        assert!(matches!(err, DbError::InvitationInvalid));
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn taken_username_does_not_consume_the_invitation(pool: PgPool) {
    let alice = admin(&pool).await;
    invite(&pool, &alice, b"token-1", Duration::days(7)).await;

    let err = users::create_with_invitation(&pool, &new_user("alice"), b"token-1").await.unwrap_err();
    assert!(matches!(err, DbError::UsernameTaken));
    assert!(invitations::is_redeemable(&pool, b"token-1").await.unwrap());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_the_creator_can_revoke_and_status_reflects_state(pool: PgPool) {
    let alice = admin(&pool).await;
    invite(&pool, &alice, b"token-1", Duration::days(7)).await;
    let bob = users::create_with_invitation(&pool, &new_user("bob"), b"token-1").await.unwrap();
    invite(&pool, &alice, b"token-2", Duration::days(7)).await;

    let list = invitations::list_created_by(&pool, alice.id).await.unwrap();
    let now = OffsetDateTime::now_utc();
    assert_eq!(list[0].status(now), InvitationStatus::Active); // newest first
    assert_eq!(list[1].status(now), InvitationStatus::Used);

    assert!(!invitations::revoke(&pool, list[0].id, bob.id).await.unwrap());
}
```

**Step 2:** `cargo test -p squirrel-db --test invitations` → FAIL.

**Step 3: Implement** (`src/invitations.rs`)
```rust
use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;

const INVITATION_COLUMNS: &str =
    "id, created_by, email_hint, max_uses, used_count, expires_at, revoked_at, created_at";
const LIST_LIMIT: i64 = 200;

#[derive(Debug, Clone, FromRow)]
pub struct InvitationRecord {
    pub id: Uuid,
    pub created_by: Uuid,
    pub email_hint: Option<String>,
    pub max_uses: i32,
    pub used_count: i32,
    pub expires_at: OffsetDateTime,
    pub revoked_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvitationStatus {
    Active,
    Used,
    Expired,
    Revoked,
}

impl InvitationRecord {
    pub fn status(&self, now: OffsetDateTime) -> InvitationStatus {
        if self.revoked_at.is_some() {
            InvitationStatus::Revoked
        } else if self.used_count >= self.max_uses {
            InvitationStatus::Used
        } else if self.expires_at <= now {
            InvitationStatus::Expired
        } else {
            InvitationStatus::Active
        }
    }
}

pub struct NewInvitation<'a> {
    pub created_by: Uuid,
    pub token_hash: &'a [u8],
    pub email_hint: Option<&'a str>,
    pub expires_at: OffsetDateTime,
}

pub async fn create(pool: &PgPool, new: &NewInvitation<'_>) -> Result<InvitationRecord, DbError> {
    let sql = format!(
        "INSERT INTO invitations (id, token_hash, created_by, email_hint, expires_at) \
         VALUES ($1, $2, $3, $4, $5) RETURNING {INVITATION_COLUMNS}"
    );
    Ok(sqlx::query_as::<_, InvitationRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(new.token_hash)
        .bind(new.created_by)
        .bind(new.email_hint)
        .bind(new.expires_at)
        .fetch_one(pool)
        .await?)
}

pub async fn list_created_by(pool: &PgPool, user_id: Uuid) -> Result<Vec<InvitationRecord>, DbError> {
    let sql = format!(
        "SELECT {INVITATION_COLUMNS} FROM invitations WHERE created_by = $1 \
         ORDER BY created_at DESC, id DESC LIMIT $2"
    );
    Ok(sqlx::query_as::<_, InvitationRecord>(&sql)
        .bind(user_id)
        .bind(LIST_LIMIT)
        .fetch_all(pool)
        .await?)
}

/// Returns `false` when the invitation does not exist, is not the caller's, or is already revoked.
pub async fn revoke(pool: &PgPool, id: Uuid, created_by: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query(
        "UPDATE invitations SET revoked_at = now() WHERE id = $1 AND created_by = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(created_by)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn is_redeemable(pool: &PgPool, token_hash: &[u8]) -> Result<bool, DbError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM invitations WHERE token_hash = $1 \
         AND revoked_at IS NULL AND expires_at > now() AND used_count < max_uses)",
    )
    .bind(token_hash)
    .fetch_one(pool)
    .await?)
}
```

**Step 4:** `cargo test -p squirrel-db` → PASS.

**Step 5: Commit** — `git add crates/squirrel-db && git commit -m "feat(db): add invitations repository"`

---

### Task 9: Sessions and instance settings repositories

**Files:**
- Create: `crates/squirrel-db/src/sessions.rs`, `crates/squirrel-db/src/settings.rs`
- Modify: `crates/squirrel-db/src/lib.rs` (`pub mod sessions; pub mod settings;`)
- Test: `crates/squirrel-db/tests/sessions.rs`

**Step 1: Failing tests**
```rust
use sqlx::PgPool;
use squirrel_core::{DisplayName, Username};
use squirrel_db::sessions::{self, NewSession};
use squirrel_db::{settings, users};
use time::{Duration, OffsetDateTime};

async fn alice(pool: &PgPool) -> users::UserRecord {
    let new_user = users::NewUser {
        username: Username::parse("alice").unwrap(),
        display_name: DisplayName::parse("Alice").unwrap(),
        password_hash: "x".into(),
    };
    users::create_first_admin(pool, &new_user).await.unwrap()
}

fn session<'a>(user_id: uuid::Uuid, hash: &'a [u8], ttl: Duration) -> NewSession<'a> {
    NewSession { user_id, token_hash: hash, csrf_token: "csrf", user_agent: Some("test"), expires_at: OffsetDateTime::now_utc() + ttl }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn finds_only_unexpired_sessions(pool: PgPool) {
    let user = alice(&pool).await;
    sessions::create(&pool, &session(user.id, b"live", Duration::days(1))).await.unwrap();
    sessions::create(&pool, &session(user.id, b"dead", Duration::seconds(-1))).await.unwrap();

    assert!(sessions::find_active(&pool, b"live").await.unwrap().is_some());
    assert!(sessions::find_active(&pool, b"dead").await.unwrap().is_none());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn delete_all_for_user_revokes_every_session(pool: PgPool) {
    let user = alice(&pool).await;
    sessions::create(&pool, &session(user.id, b"a", Duration::days(1))).await.unwrap();
    sessions::create(&pool, &session(user.id, b"b", Duration::days(1))).await.unwrap();

    assert_eq!(sessions::delete_all_for_user(&pool, user.id).await.unwrap(), 2);
    assert!(sessions::find_active(&pool, b"a").await.unwrap().is_none());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn touch_extends_expiry(pool: PgPool) {
    let user = alice(&pool).await;
    let created = sessions::create(&pool, &session(user.id, b"a", Duration::hours(1))).await.unwrap();
    let new_expiry = OffsetDateTime::now_utc() + Duration::days(30);
    sessions::touch(&pool, created.id, new_expiry).await.unwrap();

    let found = sessions::find_active(&pool, b"a").await.unwrap().unwrap();
    assert!(found.expires_at > created.expires_at);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn settings_default_and_update(pool: PgPool) {
    assert!(!settings::get(&pool).await.unwrap().invites_admin_only);
    let updated = settings::set_invites_admin_only(&pool, true).await.unwrap();
    assert!(updated.invites_admin_only);
}
```

**Step 2:** `cargo test -p squirrel-db --test sessions` → FAIL.

**Step 3: Implement**

`src/sessions.rs`:
```rust
use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;

const SESSION_COLUMNS: &str = "id, user_id, csrf_token, created_at, last_seen_at, expires_at";

#[derive(Debug, Clone, FromRow)]
pub struct SessionRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub csrf_token: String,
    pub created_at: OffsetDateTime,
    pub last_seen_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

pub struct NewSession<'a> {
    pub user_id: Uuid,
    pub token_hash: &'a [u8],
    pub csrf_token: &'a str,
    pub user_agent: Option<&'a str>,
    pub expires_at: OffsetDateTime,
}

pub async fn create(pool: &PgPool, new: &NewSession<'_>) -> Result<SessionRecord, DbError> {
    let sql = format!(
        "INSERT INTO sessions (id, token_hash, user_id, csrf_token, user_agent, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {SESSION_COLUMNS}"
    );
    Ok(sqlx::query_as::<_, SessionRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(new.token_hash)
        .bind(new.user_id)
        .bind(new.csrf_token)
        .bind(new.user_agent)
        .bind(new.expires_at)
        .fetch_one(pool)
        .await?)
}

pub async fn find_active(pool: &PgPool, token_hash: &[u8]) -> Result<Option<SessionRecord>, DbError> {
    let sql = format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE token_hash = $1 AND expires_at > now()");
    Ok(sqlx::query_as::<_, SessionRecord>(&sql)
        .bind(token_hash)
        .fetch_optional(pool)
        .await?)
}

pub async fn touch(pool: &PgPool, id: Uuid, expires_at: OffsetDateTime) -> Result<(), DbError> {
    sqlx::query("UPDATE sessions SET last_seen_at = now(), expires_at = $2 WHERE id = $1")
        .bind(id)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_by_id(pool: &PgPool, id: Uuid) -> Result<(), DbError> {
    sqlx::query("DELETE FROM sessions WHERE id = $1").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn delete_all_for_user(pool: &PgPool, user_id: Uuid) -> Result<u64, DbError> {
    let result = sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
```

`src/settings.rs`:
```rust
use sqlx::{FromRow, PgPool};

use crate::DbError;

#[derive(Debug, Clone, Copy, FromRow)]
pub struct InstanceSettings {
    pub invites_admin_only: bool,
}

pub async fn get(pool: &PgPool) -> Result<InstanceSettings, DbError> {
    Ok(sqlx::query_as::<_, InstanceSettings>("SELECT invites_admin_only FROM instance_settings")
        .fetch_one(pool)
        .await?)
}

pub async fn set_invites_admin_only(pool: &PgPool, value: bool) -> Result<InstanceSettings, DbError> {
    Ok(sqlx::query_as::<_, InstanceSettings>(
        "UPDATE instance_settings SET invites_admin_only = $1 RETURNING invites_admin_only",
    )
    .bind(value)
    .fetch_one(pool)
    .await?)
}
```

**Step 4:** `cargo test -p squirrel-db` → PASS.

**Step 5: Commit** — `git add crates/squirrel-db && git commit -m "feat(db): add sessions and instance settings repositories"`

---

### Task 10: API crate skeleton — state, error envelope, health, SPA fallback

**Files:**
- Create: `crates/squirrel-api/Cargo.toml`, `src/lib.rs`, `src/state.rs`, `src/error.rs`, `src/blocking.rs`, `src/spa.rs`, `src/routes/mod.rs`, `src/routes/health.rs`
- Test: `crates/squirrel-api/tests/common/mod.rs`, `crates/squirrel-api/tests/health.rs`

**Step 1: Manifest**
```toml
[package]
name = "squirrel-api"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
anyhow.workspace = true
axum.workspace = true
axum-extra.workspace = true
rust-embed.workspace = true
serde.workspace = true
serde_json.workspace = true
sqlx.workspace = true
squirrel-auth.workspace = true
squirrel-core.workspace = true
squirrel-db.workspace = true
subtle.workspace = true
thiserror.workspace = true
time.workspace = true
tokio.workspace = true
tower_governor.workspace = true
tracing.workspace = true
utoipa.workspace = true
utoipa-axum.workspace = true
uuid.workspace = true

[dev-dependencies]
axum-test.workspace = true
```

**Step 2: Test harness + failing tests**

`tests/common/mod.rs`:
```rust
#![allow(dead_code)]

use std::net::SocketAddr;

use axum::extract::connect_info::MockConnectInfo;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum_test::TestServer;
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::{ApiConfig, AppState};

pub const PASSWORD: &str = "correct horse battery";
pub const CSRF: HeaderName = HeaderName::from_static("x-csrf-token");

pub fn test_config() -> ApiConfig {
    ApiConfig { cookie_secure: false, login_rate_limit: None, ..ApiConfig::default() }
}

pub fn server(pool: PgPool) -> TestServer {
    server_with(pool, test_config())
}

/// Each TestServer has its own cookie jar, i.e. acts as one browser.
pub fn server_with(pool: PgPool, config: ApiConfig) -> TestServer {
    let app = squirrel_api::router(AppState::new(pool, config))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    TestServer::builder().save_cookies().build(app).expect("test server builds")
}

pub fn account(username: &str) -> Value {
    json!({ "username": username, "display_name": username, "password": PASSWORD })
}

pub fn csrf(me: &Value) -> HeaderValue {
    HeaderValue::from_str(me["csrf_token"].as_str().expect("csrf_token present")).unwrap()
}

pub async fn setup_admin(server: &TestServer, username: &str) -> Value {
    let response = server.post("/api/setup/admin").json(&account(username)).await;
    response.assert_status(StatusCode::CREATED);
    response.json()
}
```

`tests/health.rs`:
```rust
mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn healthz_reports_ok(pool: PgPool) {
    let server = common::server(pool);
    let response = server.get("/api/healthz").await;
    response.assert_status_ok();
    response.assert_json(&json!({ "status": "ok" }));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn unknown_api_route_returns_json_404(pool: PgPool) {
    let server = common::server(pool);
    let response = server.get("/api/does-not-exist").await;
    response.assert_status(StatusCode::NOT_FOUND);
    response.assert_json(&json!({ "error": { "code": "not_found", "message": "not found" } }));
}
```

**Step 3:** `cargo test -p squirrel-api` → FAIL.

**Step 4: Implement**

`src/state.rs`:
```rust
use std::sync::Arc;

use sqlx::PgPool;
use time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct RateLimit {
    /// One request token is replenished every `replenish_every_secs` seconds. Must be > 0.
    pub replenish_every_secs: u64,
    /// Requests allowed in a burst. Must be > 0.
    pub burst: u32,
}

#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub cookie_secure: bool,
    pub session_ttl: Duration,
    pub session_touch_interval: Duration,
    pub invitation_ttl: Duration,
    /// Per-IP limit on credential endpoints (login, setup, join). `None` disables it.
    pub login_rate_limit: Option<RateLimit>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            cookie_secure: true,
            session_ttl: Duration::days(30),
            session_touch_interval: Duration::hours(1),
            invitation_ttl: Duration::days(7),
            login_rate_limit: Some(RateLimit { replenish_every_secs: 2, burst: 10 }),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<ApiConfig>,
}

impl AppState {
    pub fn new(pool: PgPool, config: ApiConfig) -> Self {
        Self { pool, config: Arc::new(config) }
    }
}
```

`src/error.rs`:
```rust
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use squirrel_auth::HashError;
use squirrel_core::{DisplayNameError, PasswordError, UsernameError};
use squirrel_db::DbError;
use utoipa::ToSchema;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    Validation(String),
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("authentication required")]
    Unauthorized,
    #[error("you are not allowed to do this")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("this invitation is invalid, expired or already used")]
    InvitationInvalid,
    #[error("internal server error")]
    Internal(#[source] anyhow::Error),
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    /// Stable machine-readable code, e.g. `invalid_credentials`.
    pub code: String,
    /// Human-readable message, safe to display.
    pub message: String,
}

impl ApiError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            ApiError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_error"),
            ApiError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ApiError::InvitationInvalid => (StatusCode::GONE, "invitation_invalid"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let ApiError::Internal(source) = &self {
            tracing::error!(error = ?source, "internal error");
        }
        let (status, code) = self.status_and_code();
        let body = ErrorBody { error: ErrorDetail { code: code.to_owned(), message: self.to_string() } };
        (status, Json(body)).into_response()
    }
}

impl From<DbError> for ApiError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::NotFirstUser => ApiError::Conflict("setup is already complete".into()),
            DbError::UsernameTaken => ApiError::Conflict("this username is already taken".into()),
            DbError::InvitationInvalid => ApiError::InvitationInvalid,
            DbError::Sqlx(source) => ApiError::Internal(source.into()),
        }
    }
}

impl From<HashError> for ApiError {
    fn from(err: HashError) -> Self {
        ApiError::Internal(err.into())
    }
}

macro_rules! validation_from {
    ($($ty:ty),*) => {$(
        impl From<$ty> for ApiError {
            fn from(err: $ty) -> Self {
                ApiError::Validation(err.to_string())
            }
        }
    )*};
}
validation_from!(UsernameError, PasswordError, DisplayNameError);
```

`src/blocking.rs`:
```rust
use crate::error::ApiError;

/// Runs CPU-heavy work (Argon2) off the async executor.
pub async fn run_blocking<T, F>(work: F) -> Result<T, ApiError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|err| ApiError::Internal(err.into()))
}
```

`src/spa.rs`:
```rust
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

use crate::error::ApiError;

/// Built SvelteKit output. `allow_missing` lets the backend build before the web UI exists.
#[derive(RustEmbed)]
#[folder = "../../web/build/"]
#[allow_missing = true]
struct WebAssets;

const IMMUTABLE_PREFIX: &str = "_app/immutable/";

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path == "api" || path.starts_with("api/") {
        return ApiError::NotFound.into_response();
    }
    let asset = WebAssets::get(path).map(|file| (path, file));
    let (served_path, file) = match asset.or_else(|| WebAssets::get("index.html").map(|f| ("index.html", f))) {
        Some(found) => found,
        None => return (StatusCode::NOT_FOUND, "web UI is not built").into_response(),
    };
    let cache = if served_path.starts_with(IMMUTABLE_PREFIX) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [(header::CONTENT_TYPE, file.metadata.mimetype().to_owned()), (header::CACHE_CONTROL, cache.to_owned())],
        file.data,
    )
        .into_response()
}
```

`src/routes/health.rs`:
```rust
use axum::Json;
use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    pub status: &'static str,
}

#[utoipa::path(get, path = "/api/healthz", tag = "system", responses((status = 200, body = Health)))]
pub async fn healthz(State(state): State<AppState>) -> Result<Json<Health>, ApiError> {
    sqlx::query("SELECT 1")
        .execute(&state.pool)
        .await
        .map_err(|err| ApiError::Internal(err.into()))?;
    Ok(Json(Health { status: "ok" }))
}
```

`src/routes/mod.rs`:
```rust
pub mod health;
```

`src/lib.rs`:
```rust
//! HTTP API for Squirrel.

mod blocking;
mod error;
mod routes;
mod spa;
mod state;

pub use error::{ApiError, ErrorBody};
pub use state::{ApiConfig, AppState, RateLimit};

use axum::Router;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[derive(OpenApi)]
#[openapi(
    info(title = "Squirrel API", version = "0.1.0"),
    tags((name = "system"), (name = "setup"), (name = "auth"), (name = "invitations"), (name = "admin"))
)]
struct ApiDoc;

pub fn router(state: AppState) -> Router {
    let (router, _) = api_router(&state.config).split_for_parts();
    router.fallback(spa::serve).with_state(state)
}

/// The OpenAPI document, used to generate the committed `openapi.json`.
pub fn openapi() -> utoipa::openapi::OpenApi {
    api_router(&ApiConfig::default()).split_for_parts().1
}

fn api_router(_config: &ApiConfig) -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi()).routes(routes!(routes::health::healthz))
}
```

**Step 5:** `cargo test -p squirrel-api` → PASS.

**Step 6: Commit** — `git add crates/squirrel-api && git commit -m "feat(api): add api skeleton with error envelope and health check"`

---

### Task 11: First-admin setup endpoints and session cookie

**Files:**
- Create: `crates/squirrel-api/src/dto.rs`, `src/session.rs`, `src/routes/setup.rs`
- Modify: `src/lib.rs`, `src/routes/mod.rs`
- Test: `crates/squirrel-api/tests/setup.rs`

**Step 1: Failing tests**
```rust
mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn fresh_instance_needs_setup(pool: PgPool) {
    let server = common::server(pool);
    server.get("/api/setup/status").await.assert_json(&json!({ "needs_setup": true }));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn first_account_is_admin_and_logged_in(pool: PgPool) {
    let server = common::server(pool);
    let response = server.post("/api/setup/admin").json(&common::account("Alice")).await;
    response.assert_status(StatusCode::CREATED);
    assert!(response.cookie("squirrel_session").http_only().unwrap_or(false));

    let me: serde_json::Value = response.json();
    assert_eq!(me["username"], "alice");
    assert_eq!(me["role"], "admin");
    assert!(me["csrf_token"].as_str().unwrap().len() >= 43);
    server.get("/api/setup/status").await.assert_json(&json!({ "needs_setup": false }));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn second_setup_is_rejected(pool: PgPool) {
    let server = common::server(pool.clone());
    common::setup_admin(&server, "alice").await;

    let other_browser = common::server(pool);
    let response = other_browser.post("/api/setup/admin").json(&common::account("mallory")).await;
    response.assert_status(StatusCode::CONFLICT);
    assert_eq!(response.json::<serde_json::Value>()["error"]["code"], "conflict");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invalid_input_is_a_422_with_message(pool: PgPool) {
    let server = common::server(pool);
    let response = server
        .post("/api/setup/admin")
        .json(&json!({ "username": "al", "display_name": "Al", "password": "correct horse battery" }))
        .await;
    response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.json::<serde_json::Value>()["error"]["message"],
        "username must be between 3 and 32 characters"
    );
}
```
**Step 2:** `cargo test -p squirrel-api --test setup` → FAIL (404s).

**Step 3: Implement**

`src/dto.rs`:
```rust
use serde::{Deserialize, Serialize};
use squirrel_core::Role;
use squirrel_db::users::UserRecord;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Deserialize, ToSchema)]
pub struct AccountRequest {
    pub username: String,
    pub display_name: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeResponse {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    /// Send back as `X-CSRF-Token` on every non-GET request.
    pub csrf_token: String,
}

impl MeResponse {
    pub fn new(user: &UserRecord, csrf_token: String) -> Self {
        Self {
            id: user.id,
            username: user.username.clone(),
            display_name: user.display_name.clone(),
            role: user.role(),
            csrf_token,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetupStatus {
    pub needs_setup: bool,
}
```

`src/session.rs`:
```rust
use axum::http::HeaderMap;
use axum::http::header::USER_AGENT;
use axum_extra::extract::cookie::{Cookie, SameSite};
use squirrel_auth::{generate_token, hash_token};
use squirrel_db::sessions::{self, NewSession};
use squirrel_db::users::UserRecord;
use time::{Duration, OffsetDateTime};

use crate::dto::MeResponse;
use crate::error::ApiError;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "squirrel_session";
pub const CSRF_HEADER: &str = "x-csrf-token";
/// Browser-side lifetime; the server-side `expires_at` is authoritative.
const COOKIE_MAX_AGE: Duration = Duration::days(365);
const USER_AGENT_MAX_LEN: usize = 256;

/// Creates a session row and returns the cookie to set plus the `/me` payload.
pub async fn start(
    state: &AppState,
    user: &UserRecord,
    headers: &HeaderMap,
) -> Result<(Cookie<'static>, MeResponse), ApiError> {
    let token = generate_token();
    let csrf_token = generate_token();
    let user_agent = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|ua| ua.chars().take(USER_AGENT_MAX_LEN).collect::<String>());
    let token_hash = hash_token(&token);
    sessions::create(
        &state.pool,
        &NewSession {
            user_id: user.id,
            token_hash: &token_hash,
            csrf_token: &csrf_token,
            user_agent: user_agent.as_deref(),
            expires_at: OffsetDateTime::now_utc() + state.config.session_ttl,
        },
    )
    .await?;
    let cookie = Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.config.cookie_secure)
        .max_age(COOKIE_MAX_AGE)
        .build();
    Ok((cookie, MeResponse::new(user, csrf_token)))
}

pub fn removal() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, "")).path("/").build()
}
```

`src/routes/setup.rs`:
```rust
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum_extra::extract::CookieJar;
use squirrel_auth::hash_password;
use squirrel_core::{DisplayName, Password, Username};
use squirrel_db::users::{self, NewUser};

use crate::blocking::run_blocking;
use crate::dto::{AccountRequest, MeResponse, SetupStatus};
use crate::error::{ApiError, ErrorBody};
use crate::session;
use crate::state::AppState;

#[utoipa::path(get, path = "/api/setup/status", tag = "setup", responses((status = 200, body = SetupStatus)))]
pub async fn setup_status(State(state): State<AppState>) -> Result<Json<SetupStatus>, ApiError> {
    let needs_setup = !users::any_exists(&state.pool).await?;
    Ok(Json(SetupStatus { needs_setup }))
}

#[utoipa::path(
    post, path = "/api/setup/admin", tag = "setup",
    request_body = AccountRequest,
    responses(
        (status = 201, body = MeResponse, description = "Admin created and logged in"),
        (status = 409, body = ErrorBody, description = "Setup already complete"),
        (status = 422, body = ErrorBody),
    )
)]
pub async fn create_first_admin(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<AccountRequest>,
) -> Result<(StatusCode, CookieJar, Json<MeResponse>), ApiError> {
    if users::any_exists(&state.pool).await? {
        return Err(ApiError::Conflict("setup is already complete".into()));
    }
    let new_user = prepare_new_user(request).await?;
    let user = users::create_first_admin(&state.pool, &new_user).await?;
    let (cookie, me) = session::start(&state, &user, &headers).await?;
    Ok((StatusCode::CREATED, jar.add(cookie), Json(me)))
}

/// Validates the account fields and hashes the password off the async executor.
pub async fn prepare_new_user(request: AccountRequest) -> Result<NewUser, ApiError> {
    let username = Username::parse(&request.username)?;
    let display_name = DisplayName::parse(&request.display_name)?;
    let password = Password::parse(&request.password)?;
    let password_hash = run_blocking(move || hash_password(&password)).await??;
    Ok(NewUser { username, display_name, password_hash })
}
```
(The early `any_exists` check only avoids a wasted Argon2 hash; `create_first_admin` remains the authoritative, race-safe check.)

`src/routes/mod.rs`: add `pub mod setup;`.
`src/lib.rs`: add `mod dto; mod session;`, `pub use dto::*;` is not needed. Update `api_router`:
```rust
fn api_router(_config: &ApiConfig) -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(routes::health::healthz))
        .routes(routes!(routes::setup::setup_status))
        .routes(routes!(routes::setup::create_first_admin))
}
```

**Step 4:** `cargo test -p squirrel-api` → PASS.

**Step 5: Commit** — `git add crates/squirrel-api && git commit -m "feat(api): add first-admin setup with session cookie"`

---

### Task 12: `CurrentUser` extractor with CSRF, `/me`, logout

**Files:**
- Create: `crates/squirrel-api/src/extract.rs`, `src/routes/auth.rs`
- Modify: `src/lib.rs`, `src/routes/mod.rs`
- Test: `crates/squirrel-api/tests/session.rs`

**Step 1: Failing tests**
```rust
mod common;

use axum::http::StatusCode;
use sqlx::PgPool;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn me_requires_a_session(pool: PgPool) {
    let server = common::server(pool);
    server.get("/api/auth/me").await.assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn me_returns_the_logged_in_user(pool: PgPool) {
    let server = common::server(pool);
    let me = common::setup_admin(&server, "alice").await;
    let fetched: serde_json::Value = server.get("/api/auth/me").await.json();
    assert_eq!(fetched["username"], "alice");
    assert_eq!(fetched["csrf_token"], me["csrf_token"]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn state_changing_requests_need_the_csrf_header(pool: PgPool) {
    let server = common::server(pool);
    let me = common::setup_admin(&server, "alice").await;

    server.post("/api/auth/logout").await.assert_status(StatusCode::FORBIDDEN);
    server
        .post("/api/auth/logout")
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    server.get("/api/auth/me").await.assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn logout_all_revokes_other_browsers(pool: PgPool) {
    let first = common::server(pool.clone());
    let me = common::setup_admin(&first, "alice").await;

    let second = common::server(pool);
    second
        .post("/api/auth/login")
        .json(&serde_json::json!({ "username": "alice", "password": common::PASSWORD }))
        .await
        .assert_status_ok();

    first
        .post("/api/auth/logout-all")
        .add_header(common::CSRF, common::csrf(&me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    second.get("/api/auth/me").await.assert_status(StatusCode::UNAUTHORIZED);
}
```
(`logout_all_revokes_other_browsers` depends on login from Task 13; mark it `#[ignore]` now and remove the attribute in Task 13.)

**Step 2:** `cargo test -p squirrel-api --test session` → FAIL.

**Step 3: Implement**

`src/extract.rs`:
```rust
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use squirrel_auth::hash_token;
use squirrel_db::sessions::{self, SessionRecord};
use squirrel_db::users::{self, UserRecord};
use subtle::ConstantTimeEq;
use time::OffsetDateTime;

use crate::error::ApiError;
use crate::session::{CSRF_HEADER, SESSION_COOKIE};
use crate::state::AppState;

/// An authenticated, non-disabled user. Rejects state-changing requests whose
/// `X-CSRF-Token` header does not match the session's token.
pub struct CurrentUser {
    pub user: UserRecord,
    pub session: SessionRecord,
}

impl CurrentUser {
    pub fn require_admin(&self) -> Result<(), ApiError> {
        if self.user.role().is_admin() { Ok(()) } else { Err(ApiError::Forbidden) }
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar.get(SESSION_COOKIE).ok_or(ApiError::Unauthorized)?.value().to_owned();
        let session = sessions::find_active(&state.pool, &hash_token(&token))
            .await?
            .ok_or(ApiError::Unauthorized)?;
        let user = users::find_by_id(&state.pool, session.user_id)
            .await?
            .filter(|user| !user.is_disabled())
            .ok_or(ApiError::Unauthorized)?;

        if !parts.method.is_safe() {
            let provided = parts.headers.get(CSRF_HEADER).map(|v| v.as_bytes()).unwrap_or_default();
            if !bool::from(provided.ct_eq(session.csrf_token.as_bytes())) {
                return Err(ApiError::Forbidden);
            }
        }

        let now = OffsetDateTime::now_utc();
        if now - session.last_seen_at > state.config.session_touch_interval {
            // `touch` returns false if the session expired or was deleted meanwhile.
            if !sessions::touch(&state.pool, session.id, now + state.config.session_ttl).await? {
                return Err(ApiError::Unauthorized);
            }
        }
        Ok(Self { user, session })
    }
}
```

`src/routes/auth.rs` (login comes in Task 13):
```rust
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum_extra::extract::CookieJar;
use squirrel_db::sessions;

use crate::dto::MeResponse;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::session;
use crate::state::AppState;

#[utoipa::path(get, path = "/api/auth/me", tag = "auth",
    responses((status = 200, body = MeResponse), (status = 401, body = ErrorBody)))]
pub async fn me(current: CurrentUser) -> Json<MeResponse> {
    Json(MeResponse::new(&current.user, current.session.csrf_token.clone()))
}

#[utoipa::path(post, path = "/api/auth/logout", tag = "auth",
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub async fn logout(
    State(state): State<AppState>,
    current: CurrentUser,
    jar: CookieJar,
) -> Result<(StatusCode, CookieJar), ApiError> {
    sessions::delete_by_id(&state.pool, current.session.id).await?;
    Ok((StatusCode::NO_CONTENT, jar.remove(session::removal())))
}

#[utoipa::path(post, path = "/api/auth/logout-all", tag = "auth",
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub async fn logout_all(
    State(state): State<AppState>,
    current: CurrentUser,
    jar: CookieJar,
) -> Result<(StatusCode, CookieJar), ApiError> {
    sessions::delete_all_for_user(&state.pool, current.user.id).await?;
    Ok((StatusCode::NO_CONTENT, jar.remove(session::removal())))
}
```

Register: `mod extract;` in lib.rs; `pub mod auth;` in routes; add to `api_router`:
```rust
        .routes(routes!(routes::auth::me))
        .routes(routes!(routes::auth::logout))
        .routes(routes!(routes::auth::logout_all))
```

**Step 4:** `cargo test -p squirrel-api` → PASS (one ignored).

**Step 5: Commit** — `git add crates/squirrel-api && git commit -m "feat(api): add session extractor with csrf, me and logout"`

---

### Task 13: Login with lockout and per-IP rate limiting

**Files:**
- Create: `crates/squirrel-api/src/rate_limit.rs`
- Modify: `src/routes/auth.rs`, `src/lib.rs`, `tests/session.rs` (remove `#[ignore]`)
- Test: `crates/squirrel-api/tests/login.rs`

**Step 1: Failing tests**
```rust
mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use squirrel_api::{ApiConfig, RateLimit};

fn creds(username: &str, password: &str) -> Value {
    json!({ "username": username, "password": password })
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn login_succeeds_with_correct_password(pool: PgPool) {
    common::setup_admin(&common::server(pool.clone()), "alice").await;
    let browser = common::server(pool);
    let me: Value = browser.post("/api/auth/login").json(&creds("Alice", common::PASSWORD)).await.json();
    assert_eq!(me["username"], "alice");
    browser.get("/api/auth/me").await.assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn wrong_password_and_unknown_user_look_identical(pool: PgPool) {
    common::setup_admin(&common::server(pool.clone()), "alice").await;
    let browser = common::server(pool);

    let wrong = browser.post("/api/auth/login").json(&creds("alice", "wrong password!")).await;
    let unknown = browser.post("/api/auth/login").json(&creds("nobody", "wrong password!")).await;
    wrong.assert_status(StatusCode::UNAUTHORIZED);
    unknown.assert_status(StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.text(), unknown.text());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn account_locks_after_repeated_failures(pool: PgPool) {
    common::setup_admin(&common::server(pool.clone()), "alice").await;
    let browser = common::server(pool.clone());
    for _ in 0..5 {
        browser.post("/api/auth/login").json(&creds("alice", "wrong password!")).await;
    }
    browser
        .post("/api/auth/login")
        .json(&creds("alice", common::PASSWORD))
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    sqlx::query("UPDATE users SET locked_until = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    browser.post("/api/auth/login").json(&creds("alice", common::PASSWORD)).await.assert_status_ok();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn credential_endpoints_are_rate_limited_per_ip(pool: PgPool) {
    let config = ApiConfig {
        login_rate_limit: Some(RateLimit { replenish_every_secs: 60, burst: 3 }),
        ..common::test_config()
    };
    let browser = common::server_with(pool, config);
    for _ in 0..3 {
        browser.post("/api/auth/login").json(&creds("x", "y")).await.assert_status(StatusCode::UNAUTHORIZED);
    }
    browser.post("/api/auth/login").json(&creds("x", "y")).await.assert_status(StatusCode::TOO_MANY_REQUESTS);
}
```

**Step 2:** `cargo test -p squirrel-api --test login` → FAIL.

**Step 3: Implement**

Append to `src/routes/auth.rs`:
```rust
use axum::http::HeaderMap;
use squirrel_auth::{burn_verification_time, verify_password};
use squirrel_core::{Password, Username};
use squirrel_db::users;
use time::OffsetDateTime;

use crate::blocking::run_blocking;
use crate::dto::LoginRequest;

/// Inputs longer than this are rejected before hashing (protects the CPU).
const MAX_PASSWORD_BYTES: usize = Password::MAX_LEN * 4;

#[utoipa::path(post, path = "/api/auth/login", tag = "auth",
    request_body = LoginRequest,
    responses((status = 200, body = MeResponse), (status = 401, body = ErrorBody), (status = 429)))]
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<(CookieJar, Json<MeResponse>), ApiError> {
    if request.password.len() > MAX_PASSWORD_BYTES {
        return Err(ApiError::InvalidCredentials);
    }
    let user = match Username::parse(&request.username) {
        Ok(username) => users::find_by_username(&state.pool, &username).await?,
        Err(_) => None,
    };
    let now = OffsetDateTime::now_utc();
    let Some(user) = user.filter(|u| !u.is_disabled() && !u.is_locked(now)) else {
        run_blocking(move || burn_verification_time(&request.password)).await?;
        return Err(ApiError::InvalidCredentials);
    };

    let stored_hash = user.password_hash.clone();
    let valid = run_blocking(move || verify_password(&request.password, &stored_hash)).await??;
    if !valid {
        users::record_login_failure(&state.pool, user.id).await?;
        return Err(ApiError::InvalidCredentials);
    }
    users::record_login_success(&state.pool, user.id).await?;
    let (cookie, me) = session::start(&state, &user, &headers).await?;
    Ok((jar.add(cookie), Json(me)))
}
```
Merge the `use` lines with the existing ones at the top of the file.

`src/rate_limit.rs`:
```rust
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use utoipa_axum::router::OpenApiRouter;

use crate::state::{AppState, RateLimit};

/// Per-peer-IP limit. Behind a reverse proxy every request shares the proxy's IP,
/// making this a global limit — acceptable for a home server; per-account lockout
/// still protects individual accounts.
pub fn apply(router: OpenApiRouter<AppState>, limit: RateLimit) -> OpenApiRouter<AppState> {
    let config = GovernorConfigBuilder::default()
        .per_second(limit.replenish_every_secs)
        .burst_size(limit.burst)
        .finish()
        .expect("rate limit values are non-zero");
    router.layer(GovernorLayer::new(config))
}
```
> If the installed `tower_governor` version exposes a different constructor (e.g. `GovernorLayer { config: Arc::new(config) }` in older releases), adapt this one line — check docs.rs for the resolved version. Default key extractor is `PeerIpKeyExtractor`.

`src/lib.rs`: add `mod rate_limit;` and restructure `api_router`:
```rust
fn api_router(config: &ApiConfig) -> OpenApiRouter<AppState> {
    let credential_routes = OpenApiRouter::new()
        .routes(routes!(routes::setup::create_first_admin))
        .routes(routes!(routes::auth::login));
    let credential_routes = match config.login_rate_limit {
        Some(limit) => rate_limit::apply(credential_routes, limit),
        None => credential_routes,
    };

    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(routes::health::healthz))
        .routes(routes!(routes::setup::setup_status))
        .routes(routes!(routes::auth::me))
        .routes(routes!(routes::auth::logout))
        .routes(routes!(routes::auth::logout_all))
        .merge(credential_routes)
}
```
Remove `#[ignore]` from `logout_all_revokes_other_browsers`.

**Step 4:** `cargo test -p squirrel-api` → PASS (all, none ignored).

**Step 5: Commit** — `git add crates/squirrel-api && git commit -m "feat(api): add login with lockout and rate limiting"`

---

### Task 14: Invitations, join flow, admin settings

**Files:**
- Create: `crates/squirrel-api/src/routes/invitations.rs`, `src/routes/join.rs`, `src/routes/admin.rs`
- Modify: `src/dto.rs`, `src/routes/mod.rs`, `src/lib.rs`
- Test: `crates/squirrel-api/tests/invitations.rs`

**Step 1: Failing tests**
```rust
mod common;

use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::{Value, json};
use sqlx::PgPool;

async fn create_invitation(server: &TestServer, me: &Value) -> Value {
    let response = server
        .post("/api/invitations")
        .add_header(common::CSRF, common::csrf(me))
        .json(&json!({ "email_hint": "bob@example.com" }))
        .await;
    response.assert_status(StatusCode::CREATED);
    response.json()
}

async fn join(pool: &PgPool, token: &str, username: &str) -> (TestServer, Value) {
    let browser = common::server(pool.clone());
    let response = browser.post(&format!("/api/join/{token}")).json(&common::account(username)).await;
    response.assert_status(StatusCode::CREATED);
    let me = response.json();
    (browser, me)
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn creating_an_invitation_requires_login(pool: PgPool) {
    let server = common::server(pool);
    server.post("/api/invitations").json(&json!({})).await.assert_status(StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn invited_user_joins_once(pool: PgPool) {
    let alice = common::server(pool.clone());
    let me = common::setup_admin(&alice, "alice").await;
    let created = create_invitation(&alice, &me).await;
    let token = created["token"].as_str().unwrap();
    assert_eq!(created["invitation"]["status"], "active");

    let anonymous = common::server(pool.clone());
    anonymous.get(&format!("/api/join/{token}")).await.assert_json(&json!({ "valid": true }));

    let (bob, bob_me) = join(&pool, token, "bob").await;
    assert_eq!(bob_me["role"], "user");
    bob.get("/api/auth/me").await.assert_status_ok();

    anonymous.get(&format!("/api/join/{token}")).await.assert_json(&json!({ "valid": false }));
    let reuse = anonymous.post(&format!("/api/join/{token}")).json(&common::account("carol")).await;
    reuse.assert_status(StatusCode::GONE);

    let list: Value = alice.get("/api/invitations").await.json();
    assert_eq!(list[0]["status"], "used");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn any_user_can_invite_and_revoke_their_own(pool: PgPool) {
    let alice = common::server(pool.clone());
    let alice_me = common::setup_admin(&alice, "alice").await;
    let token = create_invitation(&alice, &alice_me).await["token"].as_str().unwrap().to_owned();
    let (bob, bob_me) = join(&pool, &token, "bob").await;

    let bobs = create_invitation(&bob, &bob_me).await;
    let id = bobs["invitation"]["id"].as_str().unwrap();

    alice
        .delete(&format!("/api/invitations/{id}"))
        .add_header(common::CSRF, common::csrf(&alice_me))
        .await
        .assert_status(StatusCode::NOT_FOUND);
    bob.delete(&format!("/api/invitations/{id}"))
        .add_header(common::CSRF, common::csrf(&bob_me))
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let token = bobs["token"].as_str().unwrap();
    common::server(pool).get(&format!("/api/join/{token}")).await.assert_json(&json!({ "valid": false }));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn admin_can_restrict_invitations_to_admins(pool: PgPool) {
    let alice = common::server(pool.clone());
    let alice_me = common::setup_admin(&alice, "alice").await;
    let token = create_invitation(&alice, &alice_me).await["token"].as_str().unwrap().to_owned();
    let (bob, bob_me) = join(&pool, &token, "bob").await;

    bob.get("/api/admin/settings").await.assert_status(StatusCode::FORBIDDEN);
    alice
        .patch("/api/admin/settings")
        .add_header(common::CSRF, common::csrf(&alice_me))
        .json(&json!({ "invites_admin_only": true }))
        .await
        .assert_json(&json!({ "invites_admin_only": true }));

    bob.post("/api/invitations")
        .add_header(common::CSRF, common::csrf(&bob_me))
        .json(&json!({}))
        .await
        .assert_status(StatusCode::FORBIDDEN);
    create_invitation(&alice, &alice_me).await;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn join_rejects_taken_username_without_consuming_invite(pool: PgPool) {
    let alice = common::server(pool.clone());
    let me = common::setup_admin(&alice, "alice").await;
    let token = create_invitation(&alice, &me).await["token"].as_str().unwrap().to_owned();

    let browser = common::server(pool.clone());
    browser
        .post(&format!("/api/join/{token}"))
        .json(&common::account("alice"))
        .await
        .assert_status(StatusCode::CONFLICT);
    join(&pool, &token, "bob").await;
}
```

**Step 2:** `cargo test -p squirrel-api --test invitations` → FAIL.

**Step 3: Implement**

Append to `src/dto.rs`:
```rust
use squirrel_db::invitations::{InvitationRecord, InvitationStatus};
use squirrel_db::settings::InstanceSettings;
use time::OffsetDateTime;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateInvitationRequest {
    /// Optional reminder of who the invitation is for. Not used to send email.
    #[serde(default)]
    pub email_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum InvitationStatusDto {
    Active,
    Used,
    Expired,
    Revoked,
}

impl From<InvitationStatus> for InvitationStatusDto {
    fn from(status: InvitationStatus) -> Self {
        match status {
            InvitationStatus::Active => Self::Active,
            InvitationStatus::Used => Self::Used,
            InvitationStatus::Expired => Self::Expired,
            InvitationStatus::Revoked => Self::Revoked,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InvitationDto {
    pub id: Uuid,
    pub email_hint: Option<String>,
    pub status: InvitationStatusDto,
    pub used_count: i32,
    pub max_uses: i32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

impl InvitationDto {
    pub fn new(record: &InvitationRecord, now: OffsetDateTime) -> Self {
        Self {
            id: record.id,
            email_hint: record.email_hint.clone(),
            status: record.status(now).into(),
            used_count: record.used_count,
            max_uses: record.max_uses,
            expires_at: record.expires_at,
            created_at: record.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedInvitation {
    pub invitation: InvitationDto,
    /// Shown once. Build the link as `<origin>/join/<token>`.
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JoinStatus {
    pub valid: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InstanceSettingsDto {
    pub invites_admin_only: bool,
}

impl From<InstanceSettings> for InstanceSettingsDto {
    fn from(settings: InstanceSettings) -> Self {
        Self { invites_admin_only: settings.invites_admin_only }
    }
}
```

`src/routes/invitations.rs`:
```rust
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use squirrel_auth::{generate_token, hash_token};
use squirrel_db::invitations::{self, NewInvitation};
use squirrel_db::settings;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::dto::{CreateInvitationRequest, CreatedInvitation, InvitationDto};
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::state::AppState;

const EMAIL_HINT_MAX_LEN: usize = 254;

#[utoipa::path(post, path = "/api/invitations", tag = "invitations",
    request_body = CreateInvitationRequest,
    responses((status = 201, body = CreatedInvitation), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_invitation(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<CreateInvitationRequest>,
) -> Result<(StatusCode, Json<CreatedInvitation>), ApiError> {
    let instance = settings::get(&state.pool).await?;
    if instance.invites_admin_only && !current.user.role().is_admin() {
        return Err(ApiError::Forbidden);
    }
    let email_hint = request.email_hint.map(|hint| hint.trim().to_owned()).filter(|hint| !hint.is_empty());
    if email_hint.as_ref().is_some_and(|hint| hint.chars().count() > EMAIL_HINT_MAX_LEN) {
        return Err(ApiError::Validation(format!("email hint must be at most {EMAIL_HINT_MAX_LEN} characters")));
    }

    let token = generate_token();
    let token_hash = hash_token(&token);
    let now = OffsetDateTime::now_utc();
    let record = invitations::create(
        &state.pool,
        &NewInvitation {
            created_by: current.user.id,
            token_hash: &token_hash,
            email_hint: email_hint.as_deref(),
            expires_at: now + state.config.invitation_ttl,
        },
    )
    .await?;
    let created = CreatedInvitation { invitation: InvitationDto::new(&record, now), token };
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(get, path = "/api/invitations", tag = "invitations",
    responses((status = 200, body = Vec<InvitationDto>), (status = 401, body = ErrorBody)))]
pub async fn list_invitations(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<InvitationDto>>, ApiError> {
    let now = OffsetDateTime::now_utc();
    let records = invitations::list_created_by(&state.pool, current.user.id).await?;
    Ok(Json(records.iter().map(|record| InvitationDto::new(record, now)).collect()))
}

#[utoipa::path(delete, path = "/api/invitations/{id}", tag = "invitations",
    params(("id" = Uuid, Path, description = "Invitation id")),
    responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn revoke_invitation(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if invitations::revoke(&state.pool, id, current.user.id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
```

`src/routes/join.rs`:
```rust
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum_extra::extract::CookieJar;
use squirrel_auth::hash_token;
use squirrel_db::{invitations, users};

use crate::dto::{AccountRequest, JoinStatus, MeResponse};
use crate::error::{ApiError, ErrorBody};
use crate::routes::setup::prepare_new_user;
use crate::session;
use crate::state::AppState;

#[utoipa::path(get, path = "/api/join/{token}", tag = "invitations",
    params(("token" = String, Path, description = "Invitation token from the link")),
    responses((status = 200, body = JoinStatus)))]
pub async fn check_invitation(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<JoinStatus>, ApiError> {
    let valid = invitations::is_redeemable(&state.pool, &hash_token(&token)).await?;
    Ok(Json(JoinStatus { valid }))
}

#[utoipa::path(post, path = "/api/join/{token}", tag = "invitations",
    params(("token" = String, Path, description = "Invitation token from the link")),
    request_body = AccountRequest,
    responses((status = 201, body = MeResponse), (status = 409, body = ErrorBody),
              (status = 410, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn accept_invitation(
    State(state): State<AppState>,
    Path(token): Path<String>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<AccountRequest>,
) -> Result<(StatusCode, CookieJar, Json<MeResponse>), ApiError> {
    let token_hash = hash_token(&token);
    // Cheap pre-check to avoid hashing a password for a dead link;
    // `create_with_invitation` remains the authoritative atomic check.
    if !invitations::is_redeemable(&state.pool, &token_hash).await? {
        return Err(ApiError::InvitationInvalid);
    }
    let new_user = prepare_new_user(request).await?;
    let user = users::create_with_invitation(&state.pool, &new_user, &token_hash).await?;
    let (cookie, me) = session::start(&state, &user, &headers).await?;
    Ok((StatusCode::CREATED, jar.add(cookie), Json(me)))
}
```

`src/routes/admin.rs`:
```rust
use axum::Json;
use axum::extract::State;
use squirrel_db::settings;

use crate::dto::InstanceSettingsDto;
use crate::error::{ApiError, ErrorBody};
use crate::extract::CurrentUser;
use crate::state::AppState;

#[utoipa::path(get, path = "/api/admin/settings", tag = "admin",
    responses((status = 200, body = InstanceSettingsDto), (status = 403, body = ErrorBody)))]
pub async fn get_settings(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<InstanceSettingsDto>, ApiError> {
    current.require_admin()?;
    Ok(Json(settings::get(&state.pool).await?.into()))
}

#[utoipa::path(patch, path = "/api/admin/settings", tag = "admin",
    request_body = InstanceSettingsDto,
    responses((status = 200, body = InstanceSettingsDto), (status = 403, body = ErrorBody)))]
pub async fn update_settings(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<InstanceSettingsDto>,
) -> Result<Json<InstanceSettingsDto>, ApiError> {
    current.require_admin()?;
    Ok(Json(settings::set_invites_admin_only(&state.pool, request.invites_admin_only).await?.into()))
}
```

`src/routes/mod.rs`: `pub mod admin; pub mod auth; pub mod health; pub mod invitations; pub mod join; pub mod setup;`

`api_router` final form:
```rust
fn api_router(config: &ApiConfig) -> OpenApiRouter<AppState> {
    let credential_routes = OpenApiRouter::new()
        .routes(routes!(routes::setup::create_first_admin))
        .routes(routes!(routes::auth::login))
        .routes(routes!(routes::join::check_invitation, routes::join::accept_invitation));
    let credential_routes = match config.login_rate_limit {
        Some(limit) => rate_limit::apply(credential_routes, limit),
        None => credential_routes,
    };

    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(routes::health::healthz))
        .routes(routes!(routes::setup::setup_status))
        .routes(routes!(routes::auth::me))
        .routes(routes!(routes::auth::logout))
        .routes(routes!(routes::auth::logout_all))
        .routes(routes!(routes::invitations::create_invitation, routes::invitations::list_invitations))
        .routes(routes!(routes::invitations::revoke_invitation))
        .routes(routes!(routes::admin::get_settings, routes::admin::update_settings))
        .merge(credential_routes)
}
```

**Step 4:** `cargo test -p squirrel-api` → PASS.

**Step 5: Commit** — `git add crates/squirrel-api && git commit -m "feat(api): add invitations, join flow and admin settings"`

---

### Task 15: `squirrel` binary — serve, migrate, openapi + OpenAPI drift test

**Files:**
- Create: `crates/squirrel/Cargo.toml`, `crates/squirrel/src/main.rs`, `openapi.json` (generated)
- Test: `crates/squirrel-api/tests/openapi.rs`

**Step 1: Manifest**
```toml
[package]
name = "squirrel"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
anyhow.workspace = true
axum.workspace = true
clap.workspace = true
sqlx.workspace = true
squirrel-api.workspace = true
squirrel-auth.workspace = true
squirrel-db.workspace = true
tokio.workspace = true
tower-http.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

**Step 2: Failing drift test** (`crates/squirrel-api/tests/openapi.rs`)
```rust
#[test]
fn committed_openapi_json_is_up_to_date() {
    let generated = squirrel_api::openapi().to_pretty_json().expect("serializable spec");
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../openapi.json");
    let committed = std::fs::read_to_string(path).unwrap_or_default();
    assert_eq!(
        committed.trim(),
        generated.trim(),
        "openapi.json is stale — run: cargo run -q -p squirrel -- openapi > openapi.json"
    );
}
```
Run: `cargo test -p squirrel-api --test openapi` → FAIL (file missing).

**Step 3: Implement `main.rs`**
```rust
use std::net::SocketAddr;

use anyhow::Context;
use clap::{ArgAction, Args, Parser, Subcommand};
use squirrel_api::{ApiConfig, AppState};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "squirrel", version, about = "Self-hosted home drive")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP server (applies pending migrations first).
    Serve(ServeArgs),
    /// Apply pending database migrations and exit.
    Migrate(DatabaseArgs),
    /// Print the OpenAPI document to stdout.
    Openapi,
}

#[derive(Args)]
struct DatabaseArgs {
    #[arg(long, env = "SQUIRREL_DATABASE_URL")]
    database_url: String,
}

#[derive(Args)]
struct ServeArgs {
    #[command(flatten)]
    database: DatabaseArgs,
    #[arg(long, env = "SQUIRREL_BIND", default_value = "0.0.0.0:8080")]
    bind: SocketAddr,
    /// Set to false only for local HTTP development.
    #[arg(long, env = "SQUIRREL_COOKIE_SECURE", default_value_t = true, action = ArgAction::Set)]
    cookie_secure: bool,
    #[arg(long, env = "SQUIRREL_LOG_JSON", default_value_t = false, action = ArgAction::Set)]
    log_json: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Openapi => print_openapi(),
        Command::Migrate(args) => {
            init_tracing(false);
            migrate(&args).await
        }
        Command::Serve(args) => {
            init_tracing(args.log_json);
            serve(args).await
        }
    }
}

fn print_openapi() -> anyhow::Result<()> {
    println!("{}", squirrel_api::openapi().to_pretty_json()?);
    Ok(())
}

async fn connect_and_migrate(args: &DatabaseArgs) -> anyhow::Result<sqlx::PgPool> {
    let pool = squirrel_db::connect(&args.database_url).await.context("connecting to Postgres")?;
    squirrel_db::migrate(&pool).await.context("applying migrations")?;
    Ok(pool)
}

async fn migrate(args: &DatabaseArgs) -> anyhow::Result<()> {
    connect_and_migrate(args).await?;
    tracing::info!("migrations applied");
    Ok(())
}

async fn serve(args: ServeArgs) -> anyhow::Result<()> {
    let pool = connect_and_migrate(&args.database).await?;
    let config = ApiConfig { cookie_secure: args.cookie_secure, ..ApiConfig::default() };
    let app = squirrel_api::router(AppState::new(pool, config)).layer(TraceLayer::new_for_http());

    // Precompute the timing-equalizer hash so the first unknown-user login isn't slower.
    tokio::task::spawn_blocking(squirrel_auth::warm_up).await.context("warming up auth")?;

    let listener = TcpListener::bind(args.bind).await.with_context(|| format!("binding {}", args.bind))?;
    tracing::info!(address = %args.bind, "squirrel is listening");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("serving HTTP")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                tracing::warn!(error = %err, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}

fn init_tracing(json: bool) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,squirrel=debug,tower_http=info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if json {
        builder.json().init();
    } else {
        builder.init();
    }
}
```
**Step 4: Generate the spec and verify**

Run:
```bash
cargo run -q -p squirrel -- openapi > openapi.json
cargo test --workspace
```
Expected: all tests PASS, including `committed_openapi_json_is_up_to_date`.

Smoke-test the server:
```bash
SQUIRREL_DATABASE_URL=postgres://squirrel:squirrel@localhost:55432/squirrel SQUIRREL_COOKIE_SECURE=false cargo run -p squirrel -- serve &
curl -s localhost:8080/api/healthz   # → {"status":"ok"}
curl -s localhost:8080/api/setup/status   # → {"needs_setup":true}
kill %1
```

**Step 5: Commit** — `git add crates/squirrel crates/squirrel-api/tests/openapi.rs openapi.json Cargo.lock && git commit -m "feat: add squirrel binary with serve, migrate and openapi commands"`

---

### Task 16: Web scaffold and generated API client

**Files:**
- Create: `web/` (SvelteKit project), `web/src/lib/api/client.ts`, `web/src/lib/api/schema.d.ts` (generated), `web/.nvmrc`
- Modify: `web/svelte.config.js`, `web/vite.config.ts`, `web/package.json`

**Step 1: Scaffold**
```bash
pnpm dlx sv create web --template minimal --types ts --no-add-ons --install pnpm
cd web
pnpm add -D @sveltejs/adapter-static openapi-typescript vitest @playwright/test
pnpm add openapi-fetch
pnpm remove @sveltejs/adapter-auto
echo 22 > .nvmrc
```
(If `sv` prompts despite the flags, choose: minimal template, TypeScript, no add-ons, pnpm.)

> **Note (SvelteKit 3):** the resolved scaffold is SvelteKit 3, which has no `svelte.config.js`: the adapter is passed to `sveltekit({ adapter })` in `vite.config.ts`, and `$lib` is replaced by the `#lib` subpath import (`package.json` `imports`). Subpath imports need explicit extensions (`#lib/api/client.ts`, `#lib/session.svelte.ts`, `#lib/api/schema.d.ts`). The code below uses `$lib`; the committed code uses `#lib`.

**Step 2: Configure static SPA output** — `web/svelte.config.js`:
```js
import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),
	kit: {
		adapter: adapter({ pages: 'build', assets: 'build', fallback: 'index.html', strict: false })
	}
};

export default config;
```

`web/vite.config.ts`:
```ts
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
	plugins: [sveltekit()],
	server: { proxy: { '/api': 'http://localhost:8080' } },
	test: { include: ['src/**/*.test.ts'] }
});
```

`web/src/routes/+layout.ts`:
```ts
export const ssr = false;
export const prerender = false;
```

Add scripts to `web/package.json`:
```json
"gen:api": "openapi-typescript ../openapi.json -o src/lib/api/schema.d.ts",
"test:unit": "vitest run",
"test:e2e": "playwright test"
```

**Step 3: Generate types and write the client**

Run: `pnpm gen:api` → creates `src/lib/api/schema.d.ts`.

`web/src/lib/api/client.ts`:
```ts
import createClient, { type Middleware } from 'openapi-fetch';
import type { paths } from './schema';

const SAFE_METHODS = new Set(['GET', 'HEAD', 'OPTIONS']);
const DEFAULT_ERROR = 'Something went wrong. Please try again.';

let csrfToken: string | null = null;

export function setCsrfToken(token: string | null): void {
	csrfToken = token;
}

const csrfMiddleware: Middleware = {
	onRequest({ request }) {
		if (csrfToken && !SAFE_METHODS.has(request.method)) {
			request.headers.set('X-CSRF-Token', csrfToken);
		}
		return request;
	}
};

export const api = createClient<paths>({ baseUrl: '', credentials: 'same-origin' });
api.use(csrfMiddleware);

/** Extracts the server's `{ error: { message } }` envelope, with sensible fallbacks. */
export function errorMessage(error: unknown, response?: Response): string {
	if (response?.status === 429) return 'Too many attempts. Wait a moment and try again.';
	if (typeof error === 'object' && error !== null && 'error' in error) {
		const detail = (error as { error?: { message?: unknown } }).error;
		if (typeof detail?.message === 'string') return detail.message;
	}
	return DEFAULT_ERROR;
}
```

**Step 4: Verify**

Run: `pnpm check && pnpm build`
Expected: no type errors; `web/build/index.html` exists.

**Step 5: Commit**
```bash
cd ..
git add web
git commit -m "feat(web): scaffold svelte spa with generated api client"
```

---

### Task 17: Client validation (TDD) and session store

**Files:**
- Create: `web/src/lib/validation.ts`, `web/src/lib/validation.test.ts`, `web/src/lib/session.svelte.ts`

**Step 1: Failing test** (`validation.test.ts`)
```ts
import { describe, expect, it } from 'vitest';
import { validateAccount } from './validation';

const valid = { username: 'alice', displayName: 'Alice', password: 'correct horse', confirmPassword: 'correct horse' };

describe('validateAccount', () => {
	it('accepts a valid account', () => {
		expect(validateAccount(valid)).toBeNull();
	});
	it('normalizes username case before checking', () => {
		expect(validateAccount({ ...valid, username: 'Alice' })).toBeNull();
	});
	it('rejects short usernames and bad characters', () => {
		expect(validateAccount({ ...valid, username: 'al' })).toMatch(/Username/);
		expect(validateAccount({ ...valid, username: 'al ice' })).toMatch(/Username/);
	});
	it('requires a display name', () => {
		expect(validateAccount({ ...valid, displayName: '   ' })).toMatch(/Display name/);
	});
	it('enforces password length and confirmation', () => {
		expect(validateAccount({ ...valid, password: 'short', confirmPassword: 'short' })).toMatch(/at least 10/);
		expect(validateAccount({ ...valid, confirmPassword: 'different!!' })).toMatch(/do not match/);
	});
});
```
Run: `cd web && pnpm test:unit` → FAIL.

**Step 2: Implement** (`validation.ts`) — mirrors the server rules in `squirrel-core`:
```ts
export const USERNAME_PATTERN = /^[a-z0-9][a-z0-9._-]{2,31}$/;
export const PASSWORD_MIN_LENGTH = 10;
export const DISPLAY_NAME_MAX_LENGTH = 64;

export interface AccountFields {
	username: string;
	displayName: string;
	password: string;
	confirmPassword: string;
}

/** Returns a user-facing error message, or null when the fields are valid. */
export function validateAccount(fields: AccountFields): string | null {
	const username = fields.username.trim().toLowerCase();
	if (!USERNAME_PATTERN.test(username)) {
		return 'Username must be 3–32 characters: lowercase letters, digits, ".", "_" or "-", starting with a letter or digit.';
	}
	const displayName = fields.displayName.trim();
	if (displayName.length === 0 || [...displayName].length > DISPLAY_NAME_MAX_LENGTH) {
		return `Display name must be between 1 and ${DISPLAY_NAME_MAX_LENGTH} characters.`;
	}
	if ([...fields.password].length < PASSWORD_MIN_LENGTH) {
		return `Password must be at least ${PASSWORD_MIN_LENGTH} characters.`;
	}
	if (fields.password !== fields.confirmPassword) {
		return 'Passwords do not match.';
	}
	return null;
}
```
Run: `pnpm test:unit` → PASS.

**Step 3: Session store** (`session.svelte.ts`)
```ts
import { api, setCsrfToken } from './api/client';
import type { components } from './api/schema';

export type Me = components['schemas']['MeResponse'];

interface SessionState {
	me: Me | null;
	needsSetup: boolean;
	loaded: boolean;
	unreachable: boolean;
}

export const session: SessionState = $state({ me: null, needsSetup: false, loaded: false, unreachable: false });

export function setMe(me: Me | null): void {
	session.me = me;
	setCsrfToken(me?.csrf_token ?? null);
}

export async function loadSession(): Promise<void> {
	try {
		const [status, me] = await Promise.all([api.GET('/api/setup/status'), api.GET('/api/auth/me')]);
		session.unreachable = !status.data;
		session.needsSetup = status.data?.needs_setup ?? false;
		setMe(me.data ?? null);
	} catch {
		session.unreachable = true;
	} finally {
		session.loaded = true;
	}
}

export async function logout(): Promise<void> {
	await api.POST('/api/auth/logout');
	setMe(null);
}
```

**Step 4:** `pnpm check` → no errors.

**Step 5: Commit** — `git add web && git commit -m "feat(web): add account validation and session store"`

---

### Task 18: Layout, setup, login, join and home pages

**Files:**
- Create: `web/src/app.css`, `web/src/lib/components/AccountForm.svelte`, `web/src/routes/+layout.svelte`, `web/src/routes/+page.svelte`, `web/src/routes/setup/+page.svelte`, `web/src/routes/login/+page.svelte`, `web/src/routes/join/[token]/+page.svelte`

**Step 1: Styles** (`app.css`, dark-first tokens)
```css
:root {
	--bg: #111315;
	--surface: #1b1e21;
	--border: #2c3136;
	--text: #e8eaed;
	--muted: #9aa0a6;
	--accent: #e0a458;
	--accent-text: #1b1e21;
	--danger: #f28b82;
	--radius: 10px;
	color-scheme: dark;
	font-family: system-ui, -apple-system, 'Segoe UI', sans-serif;
}
@media (prefers-color-scheme: light) {
	:root {
		--bg: #f6f5f2;
		--surface: #ffffff;
		--border: #dcd9d2;
		--text: #1f2328;
		--muted: #5f6368;
		--accent: #a8641c;
		--accent-text: #ffffff;
		--danger: #b3261e;
		color-scheme: light;
	}
}
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--text); }
a { color: var(--accent); }
.card { max-width: 420px; margin: 10vh auto; padding: 2rem; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); }
.page { max-width: 880px; margin: 2rem auto; padding: 0 1rem; }
.center { display: grid; place-items: center; min-height: 60vh; }
.muted { color: var(--muted); }
.error { color: var(--danger); }
form { display: grid; gap: 1rem; }
label { display: grid; gap: 0.35rem; font-size: 0.9rem; }
input { padding: 0.6rem 0.75rem; border-radius: 8px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font: inherit; }
button { padding: 0.6rem 1rem; border: 0; border-radius: 8px; background: var(--accent); color: var(--accent-text); font: inherit; font-weight: 600; cursor: pointer; }
button:disabled { opacity: 0.6; cursor: progress; }
button.link { background: none; color: var(--accent); padding: 0; font-weight: 400; }
.topbar { display: flex; justify-content: space-between; align-items: center; padding: 0.75rem 1.5rem; border-bottom: 1px solid var(--border); background: var(--surface); }
.topbar nav { display: flex; gap: 1.25rem; align-items: center; }
.brand { font-weight: 700; text-decoration: none; color: var(--text); }
table { width: 100%; border-collapse: collapse; }
th, td { text-align: left; padding: 0.5rem; border-bottom: 1px solid var(--border); }
code { word-break: break-all; }
```

**Step 2: Reusable account form** (`AccountForm.svelte`)
```svelte
<script lang="ts">
	import type { components } from '$lib/api/schema';
	import { validateAccount, type AccountFields } from '$lib/validation';

	type AccountRequest = components['schemas']['AccountRequest'];

	interface Props {
		submitLabel: string;
		/** Returns an error message to display, or null on success. */
		onsubmit: (account: AccountRequest) => Promise<string | null>;
	}

	let { submitLabel, onsubmit }: Props = $props();

	let fields: AccountFields = $state({ username: '', displayName: '', password: '', confirmPassword: '' });
	let error: string | null = $state(null);
	let busy = $state(false);

	async function handleSubmit(event: SubmitEvent) {
		event.preventDefault();
		error = validateAccount(fields);
		if (error) return;
		busy = true;
		try {
			error = await onsubmit({
				username: fields.username.trim().toLowerCase(),
				display_name: fields.displayName.trim(),
				password: fields.password
			});
		} finally {
			busy = false;
		}
	}
</script>

<form onsubmit={handleSubmit} novalidate>
	<label>Username <input name="username" autocomplete="username" bind:value={fields.username} required /></label>
	<label>Display name <input name="display_name" autocomplete="name" bind:value={fields.displayName} required /></label>
	<label>Password <input name="password" type="password" autocomplete="new-password" bind:value={fields.password} required /></label>
	<label>Confirm password <input name="confirm_password" type="password" autocomplete="new-password" bind:value={fields.confirmPassword} required /></label>
	{#if error}<p class="error" role="alert">{error}</p>{/if}
	<button type="submit" disabled={busy}>{busy ? 'Please wait…' : submitLabel}</button>
</form>
```

**Step 3: Layout with route guard** (`+layout.svelte`)
```svelte
<script lang="ts">
	import '../app.css';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { loadSession, logout, session } from '$lib/session.svelte';

	let { children } = $props();

	const PUBLIC_PREFIXES = ['/login', '/setup', '/join/'];
	const isPublic = $derived(PUBLIC_PREFIXES.some((prefix) => page.url.pathname.startsWith(prefix)));

	onMount(loadSession);

	$effect(() => {
		if (!session.loaded || session.unreachable) return;
		const path = page.url.pathname;
		if (session.needsSetup && path !== '/setup') {
			goto('/setup', { replaceState: true });
		} else if (!session.needsSetup && path === '/setup') {
			goto(session.me ? '/' : '/login', { replaceState: true });
		} else if (!session.me && !isPublic) {
			goto('/login', { replaceState: true });
		} else if (session.me && path === '/login') {
			goto('/', { replaceState: true });
		}
	});

	async function handleLogout() {
		await logout();
		await goto('/login');
	}
</script>

{#if !session.loaded}
	<main class="center"><p class="muted">Loading…</p></main>
{:else if session.unreachable}
	<main class="center"><p class="error">The Squirrel server is unreachable. Check that it is running.</p></main>
{:else}
	{#if session.me}
		<header class="topbar">
			<a href="/" class="brand">Squirrel</a>
			<nav>
				<a href="/settings/invitations">Invitations</a>
				<button class="link" onclick={handleLogout}>Log out</button>
			</nav>
		</header>
	{/if}
	{@render children()}
{/if}
```

**Step 4: Pages**

`setup/+page.svelte`:
```svelte
<script lang="ts">
	import { goto } from '$app/navigation';
	import { api, errorMessage } from '$lib/api/client';
	import type { components } from '$lib/api/schema';
	import AccountForm from '$lib/components/AccountForm.svelte';
	import { session, setMe } from '$lib/session.svelte';

	async function createAdmin(account: components['schemas']['AccountRequest']) {
		const { data, error, response } = await api.POST('/api/setup/admin', { body: account });
		if (!data) return errorMessage(error, response);
		session.needsSetup = false;
		setMe(data);
		await goto('/');
		return null;
	}
</script>

<main class="card">
	<h1>Welcome to Squirrel</h1>
	<p class="muted">Create the first account. It will administer this drive.</p>
	<AccountForm submitLabel="Create administrator account" onsubmit={createAdmin} />
</main>
```

`login/+page.svelte`:
```svelte
<script lang="ts">
	import { goto } from '$app/navigation';
	import { api, errorMessage } from '$lib/api/client';
	import { setMe } from '$lib/session.svelte';

	let username = $state('');
	let password = $state('');
	let error: string | null = $state(null);
	let busy = $state(false);

	async function handleSubmit(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = null;
		const { data, error: apiError, response } = await api.POST('/api/auth/login', {
			body: { username: username.trim(), password }
		});
		busy = false;
		if (!data) {
			error = errorMessage(apiError, response);
			return;
		}
		setMe(data);
		await goto('/');
	}
</script>

<main class="card">
	<h1>Log in</h1>
	<form onsubmit={handleSubmit}>
		<label>Username <input name="username" autocomplete="username" bind:value={username} required /></label>
		<label>Password <input name="password" type="password" autocomplete="current-password" bind:value={password} required /></label>
		{#if error}<p class="error" role="alert">{error}</p>{/if}
		<button type="submit" disabled={busy}>{busy ? 'Logging in…' : 'Log in'}</button>
	</form>
	<p class="muted">No account? Ask someone on this drive for an invitation link.</p>
</main>
```

`join/[token]/+page.svelte`:
```svelte
<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, errorMessage } from '$lib/api/client';
	import type { components } from '$lib/api/schema';
	import AccountForm from '$lib/components/AccountForm.svelte';
	import { session, setMe } from '$lib/session.svelte';

	const token = $derived(page.params.token ?? '');
	let status: 'checking' | 'valid' | 'invalid' = $state('checking');

	$effect(() => {
		const current = token;
		status = 'checking';
		api.GET('/api/join/{token}', { params: { path: { token: current } } }).then(({ data }) => {
			status = data?.valid ? 'valid' : 'invalid';
		});
	});

	async function join(account: components['schemas']['AccountRequest']) {
		const { data, error, response } = await api.POST('/api/join/{token}', {
			params: { path: { token } },
			body: account
		});
		if (!data) return errorMessage(error, response);
		setMe(data);
		await goto('/');
		return null;
	}
</script>

<main class="card">
	<h1>Join Squirrel</h1>
	{#if session.me}
		<p>You are already logged in as <strong>{session.me.display_name}</strong>. Log out to use this invitation.</p>
	{:else if status === 'checking'}
		<p class="muted">Checking your invitation…</p>
	{:else if status === 'invalid'}
		<p class="error">This invitation is invalid, expired or already used. Ask for a new link.</p>
	{:else}
		<p class="muted">You have been invited. Choose your account details.</p>
		<AccountForm submitLabel="Join Squirrel" onsubmit={join} />
	{/if}
</main>
```

`+page.svelte` (home):
```svelte
<script lang="ts">
	import { session } from '$lib/session.svelte';
</script>

{#if session.me}
	<main class="page">
		<h1>Hello, {session.me.display_name}</h1>
		<p class="muted">Your drive is ready. Files, photos and videos arrive in the next phases.</p>
	</main>
{/if}
```

**Step 5: Verify** — `pnpm check && pnpm test:unit && pnpm build` → all pass.

Manual check: run the backend (`cargo run -p squirrel -- serve` with env from Task 15) and `pnpm dev`, open http://localhost:5173 → redirected to `/setup`.

**Step 6: Commit** — `git add web && git commit -m "feat(web): add setup, login, join and home pages"`

---

### Task 19: Invitations settings page

**Files:**
- Create: `web/src/routes/settings/invitations/+page.svelte`

**Step 1: Implement**
```svelte
<script lang="ts">
	import { onMount } from 'svelte';
	import { api, errorMessage } from '$lib/api/client';
	import type { components } from '$lib/api/schema';

	type Invitation = components['schemas']['InvitationDto'];

	let invitations: Invitation[] = $state([]);
	let emailHint = $state('');
	let newLink: string | null = $state(null);
	let copied = $state(false);
	let error: string | null = $state(null);
	let busy = $state(false);

	const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' });

	async function refresh() {
		const { data, error: apiError, response } = await api.GET('/api/invitations');
		if (data) invitations = data;
		else error = errorMessage(apiError, response);
	}

	async function create(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = null;
		copied = false;
		const { data, error: apiError, response } = await api.POST('/api/invitations', {
			body: { email_hint: emailHint.trim() || null }
		});
		busy = false;
		if (!data) {
			error = errorMessage(apiError, response);
			return;
		}
		newLink = `${window.location.origin}/join/${data.token}`;
		emailHint = '';
		await refresh();
	}

	async function revoke(id: string) {
		const { error: apiError, response } = await api.DELETE('/api/invitations/{id}', { params: { path: { id } } });
		if (apiError) error = errorMessage(apiError, response);
		await refresh();
	}

	async function copyLink() {
		if (!newLink) return;
		try {
			await navigator.clipboard.writeText(newLink);
			copied = true;
		} catch {
			error = 'Could not copy automatically. Select the link and copy it manually.';
		}
	}

	onMount(refresh);
</script>

<main class="page">
	<h1>Invitations</h1>
	<p class="muted">Each link lets one person create an account. Links expire after 7 days.</p>

	<form onsubmit={create}>
		<label>Who is it for? (optional) <input bind:value={emailHint} placeholder="bob@example.com" /></label>
		<button type="submit" disabled={busy}>Create invitation</button>
	</form>

	{#if newLink}
		<section aria-live="polite">
			<p>Send this link now. It will not be shown again.</p>
			<p data-testid="invite-link"><code>{newLink}</code></p>
			<button class="link" onclick={copyLink}>{copied ? 'Copied' : 'Copy link'}</button>
		</section>
	{/if}

	{#if error}<p class="error" role="alert">{error}</p>{/if}

	<table>
		<thead><tr><th>For</th><th>Status</th><th>Expires</th><th></th></tr></thead>
		<tbody>
			{#each invitations as invitation (invitation.id)}
				<tr>
					<td>{invitation.email_hint ?? '—'}</td>
					<td>{invitation.status}</td>
					<td>{dateFormat.format(new Date(invitation.expires_at))}</td>
					<td>
						{#if invitation.status === 'active'}
							<button class="link" onclick={() => revoke(invitation.id)}>Revoke</button>
						{/if}
					</td>
				</tr>
			{:else}
				<tr><td colspan="4" class="muted">No invitations yet.</td></tr>
			{/each}
		</tbody>
	</table>
</main>
```

**Step 2: Verify** — `pnpm check && pnpm build` → pass. Manually create an invitation in the dev setup and open the link in a private window.

**Step 3: Commit** — `git add web && git commit -m "feat(web): add invitations management page"`

---

### Task 20: End-to-end onboarding test (Playwright)

**Files:**
- Create: `scripts/e2e-server.sh`, `web/playwright.config.ts`, `web/e2e/onboarding.spec.ts`

**Step 1: Server bootstrap script** (`scripts/e2e-server.sh`, `chmod +x`)
```bash
#!/usr/bin/env bash
# Starts a fresh Squirrel instance on :8099 against an empty squirrel_e2e database.
set -euo pipefail
cd "$(dirname "$0")/.."

COMPOSE=(docker compose -f deploy/docker-compose.dev.yml)
"${COMPOSE[@]}" up -d --wait postgres
"${COMPOSE[@]}" exec -T postgres dropdb -U squirrel --if-exists --force squirrel_e2e
"${COMPOSE[@]}" exec -T postgres createdb -U squirrel squirrel_e2e

(cd web && pnpm build)

export SQUIRREL_DATABASE_URL=postgres://squirrel:squirrel@localhost:55432/squirrel_e2e
export SQUIRREL_BIND=127.0.0.1:8099
export SQUIRREL_COOKIE_SECURE=false
export SQUIRREL_SETUP_TOKEN=e2e-setup-token
exec cargo run -q -p squirrel -- serve
```
> The SPA is embedded at compile time in release builds; in debug builds rust-embed reads `web/build` from disk, so building the web first is enough.

**Step 2: Playwright config** (`web/playwright.config.ts`)
```ts
import { defineConfig } from '@playwright/test';

export default defineConfig({
	testDir: 'e2e',
	workers: 1,
	use: { baseURL: 'http://localhost:8099' },
	webServer: {
		command: 'bash ../scripts/e2e-server.sh',
		url: 'http://localhost:8099/api/healthz',
		timeout: 600_000,
		reuseExistingServer: false
	}
});
```

**Step 3: The test** (`web/e2e/onboarding.spec.ts`)
```ts
import { expect, test, type Page } from '@playwright/test';

const PASSWORD = 'correct horse battery';
const SETUP_CODE = 'e2e-setup-token'; // matches SQUIRREL_SETUP_TOKEN in scripts/e2e-server.sh

async function fillAccount(page: Page, username: string, displayName: string) {
	await page.getByLabel('Username').fill(username);
	await page.getByLabel('Display name').fill(displayName);
	await page.getByLabel('Password', { exact: true }).fill(PASSWORD);
	await page.getByLabel('Confirm password').fill(PASSWORD);
}

test('first user sets up the drive and invites a second user', async ({ page, browser }) => {
	await page.goto('/');
	await expect(page).toHaveURL(/\/setup$/);
	await page.getByLabel('Setup code').fill(SETUP_CODE);
	await fillAccount(page, 'alice', 'Alice');
	await page.getByRole('button', { name: 'Create administrator account' }).click();
	await expect(page.getByRole('heading', { name: 'Hello, Alice' })).toBeVisible();

	await page.getByRole('link', { name: 'Invitations' }).click();
	await page.getByRole('button', { name: 'Create invitation' }).click();
	const link = (await page.getByTestId('invite-link').textContent())?.trim();
	expect(link).toContain('/join/');

	const bob = await (await browser.newContext()).newPage();
	await bob.goto(link!);
	await fillAccount(bob, 'bob', 'Bob');
	await bob.getByRole('button', { name: 'Join Squirrel' }).click();
	await expect(bob.getByRole('heading', { name: 'Hello, Bob' })).toBeVisible();

	const carol = await (await browser.newContext()).newPage();
	await carol.goto(link!);
	await expect(carol.getByText('invalid, expired or already used')).toBeVisible();

	await carol.goto('/setup');
	await expect(carol).toHaveURL(/\/login$/);

	await page.getByRole('button', { name: 'Log out' }).click();
	await expect(page).toHaveURL(/\/login$/);
	await page.getByLabel('Username').fill('alice');
	await page.getByLabel('Password').fill(PASSWORD);
	await page.getByRole('button', { name: 'Log in' }).click();
	await expect(page.getByRole('heading', { name: 'Hello, Alice' })).toBeVisible();
});
```

**Step 4: Run**
```bash
cd web
pnpm exec playwright install chromium
pnpm test:e2e
```
Expected: 1 passed.

**Step 5: Commit** — `git add scripts web/playwright.config.ts web/e2e && git commit -m "test(web): add onboarding end-to-end test"`

---

### Task 21: CI and README

**Files:**
- Create: `.github/workflows/ci.yml`, `README.md`

**Step 1: CI workflow**
```yaml
name: CI
on:
  push:
    branches: [main]
  pull_request:

jobs:
  rust:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: pgvector/pgvector:pg17
        env:
          POSTGRES_USER: squirrel
          POSTGRES_PASSWORD: squirrel
          POSTGRES_DB: squirrel
        ports: ["55432:5432"]
        options: >-
          --health-cmd "pg_isready -U squirrel" --health-interval 2s --health-timeout 3s --health-retries 20
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.94
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace

  web:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: web
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version-file: web/.nvmrc
          cache: pnpm
          cache-dependency-path: web/pnpm-lock.yaml
      - run: pnpm install --frozen-lockfile
      - name: Generated API client matches openapi.json
        run: pnpm gen:api && git diff --exit-code src/lib/api/schema.d.ts
      - run: pnpm check
      - run: pnpm test:unit
      - run: pnpm build
```
(`pnpm/action-setup` reads the version from `packageManager` in `web/package.json`; add `"packageManager": "pnpm@10.27.0"` if `sv` did not.)

**Step 2: README**
```markdown
# Squirrel

Self-hosted home drive: per-user storage, photo classification, documents, video streaming and phone sync.
Design: `docs/plans/2026-11-13-squirrel-home-drive-design.md`.

## Development

Requirements: Rust 1.94, Node 22, pnpm 10, Docker.

    docker compose -f deploy/docker-compose.dev.yml up -d --wait
    SQUIRREL_DATABASE_URL=postgres://squirrel:squirrel@localhost:55432/squirrel \
    SQUIRREL_COOKIE_SECURE=false cargo run -p squirrel -- serve
    cd web && pnpm install && pnpm dev   # http://localhost:5173

On first start, the server logs a one-time setup code (`first-run setup code: ...`).
Open the web UI, enter that code, and create the administrator account.
To choose the code yourself (e.g. for automated deployments), set `SQUIRREL_SETUP_TOKEN`.
Everyone else joins with an invitation link.

## Tests

    cargo test --workspace                 # needs the dev Postgres
    cd web && pnpm test:unit && pnpm test:e2e

## API contract

`openapi.json` is generated from the Rust code and committed. After changing an endpoint:

    cargo run -q -p squirrel -- openapi > openapi.json
    cd web && pnpm gen:api
```

**Step 3: Verify locally** — `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && (cd web && pnpm check && pnpm test:unit && pnpm build)` → all green.

**Step 4: Commit** — `git add .github README.md web/package.json && git commit -m "ci: add rust and web checks, document dev setup"`

---

## Done when

- `cargo test --workspace` green (core, auth, db, api — including bootstrap race, single-use invites, lockout, rate limit, CSRF, OpenAPI drift).
- `pnpm test:unit` and `pnpm test:e2e` green.
- Manual: fresh DB → `/setup` → admin → create invite → join in a private window → log out / log in.
