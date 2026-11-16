//! `squirrel admin …`: operator commands run on the server.

use std::path::PathBuf;

use clap::Subcommand;
use sqlx::PgPool;
use squirrel_core::Username;
use squirrel_jobs::admin::{self, FailedJob, KindSummary, RetryOutcome};
use time::format_description::well_known::Rfc3339;

#[derive(Subcommand)]
pub enum AdminCommand {
    /// Set a user's storage quota: a number of bytes, or `unlimited`.
    SetQuota {
        username: String,
        #[arg(value_parser = parse_quota)]
        quota: Quota,
    },
    /// Bind an existing data directory to this database. Needed once for a
    /// data directory created before installation markers (or restored without
    /// its `.squirrel-instance`): the server refuses to start until then.
    AdoptDataDir {
        #[arg(long, env = "SQUIRREL_DATA_DIR")]
        data_dir: PathBuf,
        /// Bind even if the database knows none of the stored blobs.
        #[arg(long)]
        force: bool,
    },
    /// The background job queue: counts per kind and the 20 most recent failures.
    Jobs {
        #[command(subcommand)]
        action: Option<JobsAction>,
    },
    /// The media pipeline (thumbnails, metadata).
    Media {
        #[command(subcommand)]
        action: MediaAction,
    },
    /// The offline reverse-geocoding dataset (no database needed).
    Geodata {
        #[command(subcommand)]
        action: GeodataAction,
    },
}

#[derive(Subcommand)]
pub enum GeodataAction {
    /// Join GeoNames dumps into the reverse geocoder's data file.
    Build {
        /// GeoNames `cities500.txt`.
        #[arg(long)]
        cities: PathBuf,
        /// GeoNames `admin1CodesASCII.txt`.
        #[arg(long)]
        admin1: PathBuf,
        /// GeoNames `countryInfo.txt`.
        #[arg(long)]
        countries: PathBuf,
        /// When the dumps were downloaded (recorded in the header), e.g. `2026-11-14`.
        #[arg(long)]
        date: String,
        /// The file to write (must not exist).
        #[arg(long)]
        out: PathBuf,
    },
    /// Print the pinned snapshot as a `shasum -a 256 -c` line: `<sha256>  <file name>`.
    Pinned,
}

#[derive(Subcommand)]
pub enum MediaAction {
    /// Queue processing of every stored file not processed yet (and of
    /// thumbnails made with older settings, and places not named with the
    /// current geodata). A worker runs it.
    Backfill,
}

#[derive(Subcommand)]
pub enum JobsAction {
    /// Queue a failed job again with fresh attempts.
    Retry { id: i64 },
    /// Queue a job that does nothing: it disappears once a worker is running.
    Ping,
}

/// Failures printed by `squirrel admin jobs`.
const SHOWN_FAILURES: i64 = 20;

/// `None` is unlimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quota(pub Option<i64>);

pub fn parse_quota(raw: &str) -> Result<Quota, String> {
    if raw.eq_ignore_ascii_case("unlimited") {
        return Ok(Quota(None));
    }
    raw.parse::<i64>()
        .ok()
        .filter(|bytes| *bytes >= 0)
        .map(|bytes| Quota(Some(bytes)))
        .ok_or_else(|| format!("expected a number of bytes or `unlimited`, got {raw:?}"))
}

/// Runs `command` if it needs no database; otherwise hands it back.
pub fn run_offline(command: AdminCommand) -> anyhow::Result<Option<AdminCommand>> {
    match command {
        AdminCommand::Geodata { action } => {
            crate::geodata::run(action)?;
            Ok(None)
        }
        other => Ok(Some(other)),
    }
}

pub async fn run(pool: &PgPool, command: AdminCommand) -> anyhow::Result<()> {
    match command {
        AdminCommand::SetQuota { username, quota } => {
            let username = Username::parse(&username)?;
            if !squirrel_db::users::set_quota(pool, &username, quota.0).await? {
                anyhow::bail!("no user named {}", username.as_str());
            }
            match quota.0 {
                Some(bytes) => println!("quota for {} set to {bytes} bytes", username.as_str()),
                None => println!("quota for {} removed (unlimited)", username.as_str()),
            }
            Ok(())
        }
        AdminCommand::AdoptDataDir { data_dir, force } => {
            println!("{}", crate::data_dir::adopt(pool, &data_dir, force).await?);
            Ok(())
        }
        AdminCommand::Jobs { action } => jobs(pool, action).await,
        AdminCommand::Geodata { action } => crate::geodata::run(action),
        AdminCommand::Media {
            action: MediaAction::Backfill,
        } => {
            if crate::worker::enqueue_backfill(pool).await? {
                println!("media backfill queued");
            } else {
                println!("a media backfill is already queued or running");
            }
            Ok(())
        }
    }
}

async fn jobs(pool: &PgPool, action: Option<JobsAction>) -> anyhow::Result<()> {
    match action {
        None => {
            let kinds = admin::summary(pool).await?;
            let failures = admin::failed(pool, SHOWN_FAILURES).await?;
            print!("{}", format_jobs(&kinds, &failures));
        }
        Some(JobsAction::Retry { id }) => match admin::retry_failed(pool, id).await? {
            RetryOutcome::Requeued => println!("job {id} queued again"),
            RetryOutcome::AlreadyQueued => {
                println!("an equivalent job is already queued; removed failed job {id}")
            }
            RetryOutcome::NotFound => anyhow::bail!("no failed job with id {id}"),
        },
        Some(JobsAction::Ping) => {
            let mut conn = pool.acquire().await?;
            squirrel_jobs::enqueue(&mut conn, &crate::worker::noop_job()).await?;
            println!(
                "queued a noop job; it disappears from `squirrel admin jobs` once a worker runs it"
            );
        }
    }
    Ok(())
}

fn format_jobs(kinds: &[KindSummary], failures: &[FailedJob]) -> String {
    let mut out = String::new();
    if kinds.is_empty() {
        out.push_str("no jobs\n");
    } else {
        out.push_str(&format!(
            "{:<24} {:>8} {:>8} {:>8}\n",
            "KIND", "QUEUED", "RUNNING", "FAILED"
        ));
        for k in kinds {
            out.push_str(&format!(
                "{:<24} {:>8} {:>8} {:>8}\n",
                k.kind, k.queued, k.running, k.failed
            ));
        }
    }
    if !failures.is_empty() {
        out.push_str("\nrecent failures (`squirrel admin jobs retry <id>`):\n");
        for f in failures {
            let when = f.updated_at.format(&Rfc3339).unwrap_or_default();
            let error = f.last_error.as_deref().unwrap_or("-").replace('\n', " ");
            out.push_str(&format!(
                "{:>8}  {}  {}  attempts {}  {}\n",
                f.id, when, f.kind, f.attempts, error
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn job_listings_show_counts_then_failures_on_one_line_each() {
        let kinds = [KindSummary {
            kind: "thumbnails".into(),
            queued: 3,
            running: 1,
            failed: 2,
        }];
        let failures = [FailedJob {
            id: 42,
            kind: "thumbnails".into(),
            attempts: 5,
            last_error: Some("decode\nfailed".into()),
            updated_at: datetime!(2026-11-14 10:00 UTC),
        }];
        let out = format_jobs(&kinds, &failures);
        assert!(
            out.contains("thumbnails                      3        1        2"),
            "{out}"
        );
        assert!(
            out.contains("      42  2026-11-14T10:00:00Z  thumbnails  attempts 5  decode failed"),
            "{out}"
        );
        assert_eq!(format_jobs(&[], &[]), "no jobs\n");
    }
}
