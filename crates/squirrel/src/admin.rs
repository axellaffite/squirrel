//! `squirrel admin …`: operator commands run on the server.

use clap::Subcommand;
use sqlx::PgPool;
use squirrel_core::Username;

#[derive(Subcommand)]
pub enum AdminCommand {
    /// Set a user's storage quota: a number of bytes, or `unlimited`.
    SetQuota {
        username: String,
        #[arg(value_parser = parse_quota)]
        quota: Quota,
    },
}

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
    }
}
