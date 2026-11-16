mod common;

use sqlx::PgPool;
use squirrel_db::devices::{self, NewAccess, NewDevice, Refresh, Rotation};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

fn in_(duration: Duration) -> OffsetDateTime {
    OffsetDateTime::now_utc() + duration
}

fn access(hash: &[u8]) -> NewAccess<'_> {
    NewAccess {
        hash,
        expires_at: in_(Duration::minutes(15)),
    }
}

fn phone<'a>(owner_id: Uuid, refresh: &'a [u8], access_hash: &'a [u8]) -> NewDevice<'a> {
    NewDevice {
        owner_id,
        name: "Pixel 8",
        platform: "android",
        model: Some("Pixel 8"),
        app_version: "0.1.0",
        refresh_hash: refresh,
        refresh_expires_at: in_(Duration::days(90)),
        access: access(access_hash),
    }
}

fn refresh<'a>(presented: &'a [u8], new_refresh: &'a [u8], new_access: &'a [u8]) -> Refresh<'a> {
    Refresh {
        presented,
        new_refresh,
        rotation_salt: b"salt",
        refresh_expires_at: in_(Duration::days(90)),
        access: access(new_access),
    }
}

async fn retired_count(pool: &PgPool, device: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM device_retired_refresh WHERE device_id = $1")
        .bind(device)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn create_stores_the_device_and_its_first_access_token(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let device = devices::create(&pool, &phone(alice.id, b"r1", b"a1"))
        .await
        .unwrap();
    assert_eq!(device.owner_id, alice.id);
    assert_eq!(device.name, "Pixel 8");
    assert_eq!(device.platform, "android");
    assert_eq!(device.model.as_deref(), Some("Pixel 8"));
    assert!(device.revoked_at.is_none());

    let found = devices::find_by_access_hash(&pool, b"a1").await.unwrap();
    assert_eq!(found.map(|d| d.id), Some(device.id));
    assert!(
        devices::find_by_access_hash(&pool, b"unknown")
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn expired_or_revoked_access_tokens_find_nothing(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let mut expired = phone(alice.id, b"r1", b"old");
    expired.access.expires_at = in_(Duration::seconds(-1));
    devices::create(&pool, &expired).await.unwrap();
    assert!(
        devices::find_by_access_hash(&pool, b"old")
            .await
            .unwrap()
            .is_none()
    );

    let device = devices::create(&pool, &phone(alice.id, b"r2", b"live"))
        .await
        .unwrap();
    assert!(devices::revoke(&pool, alice.id, device.id).await.unwrap());
    assert!(
        devices::find_by_access_hash(&pool, b"live")
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn refreshing_with_the_current_token_rotates_it(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let device = devices::create(&pool, &phone(alice.id, b"r1", b"a1"))
        .await
        .unwrap();
    let rotated = devices::rotate_refresh(&pool, &refresh(b"r1", b"r2", b"a2"))
        .await
        .unwrap();
    let Rotation::Rotated(after) = rotated else {
        panic!("expected a rotation, got {rotated:?}");
    };
    assert_eq!(after.id, device.id);
    assert!(after.refresh_expires_at >= device.refresh_expires_at);
    // The first access token stays valid until it expires; the new one works.
    assert!(
        devices::find_by_access_hash(&pool, b"a1")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        devices::find_by_access_hash(&pool, b"a2")
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(retired_count(&pool, device.id).await, 0);

    // r2 is current, r1 previous: r2 → r3 retires r1.
    let rotated = devices::rotate_refresh(&pool, &refresh(b"r2", b"r3", b"a3"))
        .await
        .unwrap();
    assert!(matches!(rotated, Rotation::Rotated(_)));
    assert_eq!(retired_count(&pool, device.id).await, 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_previous_token_replays_within_the_grace_window(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let device = devices::create(&pool, &phone(alice.id, b"r1", b"a1"))
        .await
        .unwrap();
    devices::rotate_refresh(&pool, &refresh(b"r1", b"r2", b"a2"))
        .await
        .unwrap();
    // The client never saw r2 (or a second request raced the first): r1 again.
    let replayed = devices::rotate_refresh(&pool, &refresh(b"r1", b"r3", b"a3"))
        .await
        .unwrap();
    let Rotation::Replayed {
        device: after,
        rotation_salt,
    } = replayed
    else {
        panic!("expected a replay, got {replayed:?}");
    };
    assert_eq!(after.id, device.id);
    assert_eq!(rotation_salt, b"salt");
    // Not rotated again: r2 stays current, nothing retired, a3 works.
    assert_eq!(retired_count(&pool, device.id).await, 0);
    assert!(
        devices::find_by_access_hash(&pool, b"a3")
            .await
            .unwrap()
            .is_some()
    );
    let next = devices::rotate_refresh(&pool, &refresh(b"r2", b"r4", b"a4"))
        .await
        .unwrap();
    assert!(matches!(next, Rotation::Rotated(_)));
    // The replay's own candidate token was never stored.
    let never_stored = devices::rotate_refresh(&pool, &refresh(b"r3", b"x", b"y"))
        .await
        .unwrap();
    assert!(matches!(never_stored, Rotation::Unknown));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_previous_token_after_the_grace_window_is_reuse(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let device = devices::create(&pool, &phone(alice.id, b"r1", b"a1"))
        .await
        .unwrap();
    devices::rotate_refresh(&pool, &refresh(b"r1", b"r2", b"a2"))
        .await
        .unwrap();
    sqlx::query("UPDATE devices SET rotated_at = now() - interval '31 seconds'")
        .execute(&pool)
        .await
        .unwrap();
    let late = devices::rotate_refresh(&pool, &refresh(b"r1", b"r3", b"a3"))
        .await
        .unwrap();
    assert!(matches!(late, Rotation::Reused { device_id, .. } if device_id == device.id));
    assert!(devices::list_for_owner(&pool, alice.id).await.unwrap()[0].is_revoked());
    assert!(
        devices::find_by_access_hash(&pool, b"a2")
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn concurrent_refreshes_with_one_token_leave_the_device_healthy(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let device = devices::create(&pool, &phone(alice.id, b"r1", b"a1"))
        .await
        .unwrap();
    // Two connections of the pool, racing on the device row.
    let left = Refresh {
        rotation_salt: b"salt-left",
        ..refresh(b"r1", b"left", b"a-left")
    };
    let right = Refresh {
        rotation_salt: b"salt-right",
        ..refresh(b"r1", b"right", b"a-right")
    };
    let (first, second) = tokio::join!(
        devices::rotate_refresh(&pool, &left),
        devices::rotate_refresh(&pool, &right),
    );
    let outcomes = [first.unwrap(), second.unwrap()];
    let rotated = outcomes
        .iter()
        .filter(|o| matches!(o, Rotation::Rotated(_)))
        .count();
    let replayed = outcomes
        .iter()
        .filter(|o| matches!(o, Rotation::Replayed { .. }))
        .count();
    assert_eq!((rotated, replayed), (1, 1), "{outcomes:?}");
    assert!(!devices::list_for_owner(&pool, alice.id).await.unwrap()[0].is_revoked());
    for hash in [b"a-left".as_slice(), b"a-right"] {
        assert!(
            devices::find_by_access_hash(&pool, hash)
                .await
                .unwrap()
                .is_some()
        );
    }
    // Both callers derive the same current token from r1 and the stored salt:
    // the replay returns the salt of the rotation that won.
    let current: Vec<u8> = sqlx::query_scalar("SELECT refresh_hash FROM devices WHERE id = $1")
        .bind(device.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let expected_salt: &[u8] = if current == b"left" {
        b"salt-left"
    } else {
        b"salt-right"
    };
    let salt: Vec<u8> = sqlx::query_scalar("SELECT rotation_salt FROM devices WHERE id = $1")
        .bind(device.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    for outcome in &outcomes {
        if let Rotation::Replayed { rotation_salt, .. } = outcome {
            assert_eq!(rotation_salt, &salt);
            assert_eq!(rotation_salt.as_slice(), expected_salt);
        }
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn presenting_a_retired_token_revokes_the_device(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let device = devices::create(&pool, &phone(alice.id, b"r1", b"a1"))
        .await
        .unwrap();
    devices::rotate_refresh(&pool, &refresh(b"r1", b"r2", b"a2"))
        .await
        .unwrap();
    devices::rotate_refresh(&pool, &refresh(b"r2", b"r3", b"a3"))
        .await
        .unwrap();

    let reused = devices::rotate_refresh(&pool, &refresh(b"r1", b"r9", b"a9"))
        .await
        .unwrap();
    assert!(matches!(
        reused,
        Rotation::Reused { device_id, owner_id } if device_id == device.id && owner_id == alice.id
    ));
    let listed = devices::list_for_owner(&pool, alice.id).await.unwrap();
    assert!(listed[0].revoked_at.is_some());
    let tokens: i64 = sqlx::query_scalar("SELECT count(*) FROM device_access_tokens")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tokens, 0);
    // The current refresh token no longer works either.
    let after = devices::rotate_refresh(&pool, &refresh(b"r3", b"r4", b"a4"))
        .await
        .unwrap();
    assert!(matches!(after, Rotation::Unknown));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_unknown_token_is_unknown(pool: PgPool) {
    common::admin(&pool).await;
    let rotation = devices::rotate_refresh(&pool, &refresh(b"nope", b"r2", b"a2"))
        .await
        .unwrap();
    assert!(matches!(rotation, Rotation::Unknown));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_expired_refresh_token_is_refused_without_touching_the_device(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let mut new = phone(alice.id, b"r1", b"a1");
    new.refresh_expires_at = in_(Duration::seconds(-1));
    let device = devices::create(&pool, &new).await.unwrap();
    let rotation = devices::rotate_refresh(&pool, &refresh(b"r1", b"r2", b"a2"))
        .await
        .unwrap();
    assert!(matches!(rotation, Rotation::Expired));
    let listed = devices::list_for_owner(&pool, alice.id).await.unwrap();
    assert_eq!(listed[0].refresh_expires_at, device.refresh_expires_at);
    assert!(listed[0].revoked_at.is_none());
    assert!(
        devices::find_by_access_hash(&pool, b"a2")
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn listing_is_per_owner_live_first(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let old = devices::create(&pool, &phone(alice.id, b"r1", b"a1"))
        .await
        .unwrap();
    let new = devices::create(&pool, &phone(alice.id, b"r2", b"a2"))
        .await
        .unwrap();
    devices::create(&pool, &phone(bob.id, b"r3", b"a3"))
        .await
        .unwrap();
    devices::revoke(&pool, alice.id, old.id).await.unwrap();

    let listed: Vec<Uuid> = devices::list_for_owner(&pool, alice.id)
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(listed, vec![new.id, old.id]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn revoke_is_idempotent_and_owner_scoped(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let device = devices::create(&pool, &phone(alice.id, b"r1", b"a1"))
        .await
        .unwrap();
    assert!(!devices::revoke(&pool, bob.id, device.id).await.unwrap());
    assert!(
        devices::find_by_access_hash(&pool, b"a1")
            .await
            .unwrap()
            .is_some()
    );
    assert!(devices::revoke(&pool, alice.id, device.id).await.unwrap());
    let first = devices::list_for_owner(&pool, alice.id).await.unwrap()[0].revoked_at;
    assert!(devices::revoke(&pool, alice.id, device.id).await.unwrap());
    let second = devices::list_for_owner(&pool, alice.id).await.unwrap()[0].revoked_at;
    assert_eq!(first, second, "a second revoke keeps the first time");
    assert!(
        !devices::revoke(&pool, alice.id, Uuid::now_v7())
            .await
            .unwrap()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn revoke_all_counts_the_live_devices_of_one_user(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let first = devices::create(&pool, &phone(alice.id, b"r1", b"a1"))
        .await
        .unwrap();
    devices::create(&pool, &phone(alice.id, b"r2", b"a2"))
        .await
        .unwrap();
    devices::create(&pool, &phone(alice.id, b"r3", b"a3"))
        .await
        .unwrap();
    devices::create(&pool, &phone(bob.id, b"r4", b"a4"))
        .await
        .unwrap();
    devices::revoke(&pool, alice.id, first.id).await.unwrap();

    assert_eq!(
        devices::revoke_all_for_user(&mut pool.acquire().await.unwrap(), alice.id)
            .await
            .unwrap(),
        2
    );
    for hash in [b"a2", b"a3"] {
        assert!(
            devices::find_by_access_hash(&pool, hash)
                .await
                .unwrap()
                .is_none()
        );
    }
    assert!(
        devices::find_by_access_hash(&pool, b"a4")
            .await
            .unwrap()
            .is_some()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn delete_expired_tokens_keeps_live_ones(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let mut old = phone(alice.id, b"r1", b"old");
    old.access.expires_at = in_(Duration::seconds(-1));
    devices::create(&pool, &old).await.unwrap();
    devices::create(&pool, &phone(alice.id, b"r2", b"live"))
        .await
        .unwrap();

    assert_eq!(devices::delete_expired_tokens(&pool).await.unwrap(), 1);
    assert!(
        devices::find_by_access_hash(&pool, b"live")
            .await
            .unwrap()
            .is_some()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn stale_devices_and_old_retired_tokens_are_deleted(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let revoked = devices::create(&pool, &phone(alice.id, b"r1", b"a1"))
        .await
        .unwrap();
    let mut idle = phone(alice.id, b"r2", b"a2");
    idle.refresh_expires_at = in_(Duration::days(-31));
    devices::create(&pool, &idle).await.unwrap();
    let live = devices::create(&pool, &phone(alice.id, b"r3", b"a3"))
        .await
        .unwrap();
    devices::revoke(&pool, alice.id, revoked.id).await.unwrap();
    sqlx::query("UPDATE devices SET revoked_at = now() - interval '31 days' WHERE id = $1")
        .bind(revoked.id)
        .execute(&pool)
        .await
        .unwrap();
    devices::rotate_refresh(&pool, &refresh(b"r3", b"r4", b"a4"))
        .await
        .unwrap();
    devices::rotate_refresh(&pool, &refresh(b"r4", b"r5", b"a5"))
        .await
        .unwrap();
    sqlx::query("UPDATE device_retired_refresh SET retired_at = now() - interval '91 days'")
        .execute(&pool)
        .await
        .unwrap();

    let cutoff = in_(Duration::days(-30));
    assert_eq!(devices::delete_stale(&pool, cutoff).await.unwrap(), 2);
    let left: Vec<Uuid> = devices::list_for_owner(&pool, alice.id)
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(left, vec![live.id]);
    assert_eq!(
        devices::prune_retired(&pool, in_(Duration::days(-90)))
            .await
            .unwrap(),
        1
    );
    assert_eq!(retired_count(&pool, live.id).await, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn touch_and_rename(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let device = devices::create(&pool, &phone(alice.id, b"r1", b"a1"))
        .await
        .unwrap();
    sqlx::query("UPDATE devices SET last_seen_at = now() - interval '2 hours'")
        .execute(&pool)
        .await
        .unwrap();
    devices::touch(&pool, device.id).await.unwrap();
    let touched = devices::find_for_owner(&pool, alice.id, device.id)
        .await
        .unwrap()
        .unwrap();
    assert!(touched.last_seen_at > OffsetDateTime::now_utc() - Duration::minutes(1));

    let renamed = devices::rename(&pool, alice.id, device.id, "Work phone")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(renamed.name, "Work phone");
    assert!(
        devices::rename(&pool, bob.id, device.id, "Mine")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        devices::find_for_owner(&pool, bob.id, device.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn deleting_the_user_or_the_backup_folder_cascades(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let photos = common::root(&pool, bob.id, "Photos").await;
    let folder = common::raw_folder(&pool, bob.id, photos.id, "Pixel 8").await;
    let device = devices::create(&pool, &phone(bob.id, b"r1", b"a1"))
        .await
        .unwrap();
    devices::set_backup_folder(&pool, device.id, folder)
        .await
        .unwrap();
    assert_eq!(
        devices::find_for_owner(&pool, bob.id, device.id)
            .await
            .unwrap()
            .unwrap()
            .backup_folder_id,
        Some(folder)
    );
    sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(folder)
        .execute(&pool)
        .await
        .unwrap();
    let after = devices::find_for_owner(&pool, bob.id, device.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.backup_folder_id, None);
    assert_eq!(after.owner_id, bob.id);

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(bob.id)
        .execute(&pool)
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM devices")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_backup_folder_of_another_owner_is_refused(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let bob = common::invited(&pool, &alice, "bob").await;
    let photos = common::root(&pool, alice.id, "Photos").await;
    let device = devices::create(&pool, &phone(bob.id, b"r1", b"a1"))
        .await
        .unwrap();
    assert!(
        devices::set_backup_folder(&pool, device.id, photos.id)
            .await
            .is_err()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn revoke_all_follows_the_callers_transaction(pool: PgPool) {
    let alice = common::admin(&pool).await;
    devices::create(&pool, &phone(alice.id, b"r1", b"a1"))
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        devices::revoke_all_for_user(&mut tx, alice.id)
            .await
            .unwrap(),
        1
    );
    squirrel_db::sessions::delete_all_for_user(&mut *tx, alice.id)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert!(
        devices::find_by_access_hash(&pool, b"a1")
            .await
            .unwrap()
            .is_some()
    );
}
