mod common;

use common::{admin, invited, photo, raw_restore, raw_trash, root};
use sqlx::PgPool;
use squirrel_core::AlbumName;
use squirrel_db::access;
use squirrel_db::albums::{self, AlbumUpdate, CoverChange, Remover, UpdateOutcome};
use time::macros::datetime;
use uuid::Uuid;

fn name(raw: &str) -> AlbumName {
    AlbumName::parse(raw).unwrap()
}

async fn item_names(pool: &PgPool, _owner: Uuid, album: Uuid) -> Vec<String> {
    albums::items(pool, album, None, 100)
        .await
        .unwrap()
        .items
        .into_iter()
        .map(|a| a.name)
        .collect()
}

async fn album_names(pool: &PgPool, owner: Uuid) -> Vec<String> {
    albums::list(pool, owner)
        .await
        .unwrap()
        .into_iter()
        .map(|a| a.name)
        .collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn creates_lists_renames_and_deletes(pool: PgPool) {
    let alice = admin(&pool).await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    let old = photo(
        &pool,
        alice.id,
        photos,
        "old.jpg",
        datetime!(2023-01-01 10:00),
    )
    .await;
    let new = photo(
        &pool,
        alice.id,
        photos,
        "new.jpg",
        datetime!(2024-01-01 10:00),
    )
    .await;
    let summer = albums::create(&pool, alice.id, &name("Summer"), &[old, new])
        .await
        .unwrap();
    let empty = albums::create(&pool, alice.id, &name("Empty"), &[])
        .await
        .unwrap();
    assert_eq!(album_names(&pool, alice.id).await, ["Empty", "Summer"]);

    let found = albums::find(&pool, alice.id, summer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((found.item_count, found.cover_id), (2, Some(new)));
    let none = albums::find(&pool, alice.id, empty).await.unwrap().unwrap();
    assert_eq!((none.item_count, none.cover_id), (0, None));

    let rename = AlbumUpdate {
        name: Some(name("Summer 2024")),
        ..AlbumUpdate::default()
    };
    assert_eq!(
        albums::update(&pool, summer, alice.id, &rename)
            .await
            .unwrap(),
        UpdateOutcome::Updated
    );
    assert_eq!(album_names(&pool, alice.id).await, ["Summer 2024", "Empty"]);

    assert!(albums::delete(&pool, summer, alice.id).await.unwrap());
    assert!(!albums::delete(&pool, summer, alice.id).await.unwrap());
    assert_eq!(album_names(&pool, alice.id).await, ["Empty"]);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE node_id = ANY($1)")
        .bind([old, new])
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 2, "photos stay");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn adding_is_idempotent_and_skips_what_is_not_the_owners_live_photo(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    let bob_photos = root(&pool, bob.id, "Photos").await.id;
    let a = photo(
        &pool,
        alice.id,
        photos,
        "a.jpg",
        datetime!(2024-07-01 10:00),
    )
    .await;
    let b = photo(
        &pool,
        alice.id,
        photos,
        "b.jpg",
        datetime!(2024-07-02 10:00),
    )
    .await;
    let trashed = photo(
        &pool,
        alice.id,
        photos,
        "t.jpg",
        datetime!(2024-07-03 10:00),
    )
    .await;
    raw_trash(&pool, trashed).await;
    let bobs = photo(
        &pool,
        bob.id,
        bob_photos,
        "bob.jpg",
        datetime!(2024-07-04 10:00),
    )
    .await;
    let album = albums::create(&pool, alice.id, &name("A"), &[])
        .await
        .unwrap();
    let before = albums::find(&pool, alice.id, album).await.unwrap().unwrap();

    let ids = [a, b, trashed, bobs, photos, Uuid::now_v7()];
    assert_eq!(
        albums::add_items(&pool, album, alice.id, &ids)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        albums::add_items(&pool, album, alice.id, &ids)
            .await
            .unwrap(),
        0
    );
    assert_eq!(item_names(&pool, alice.id, album).await, ["b.jpg", "a.jpg"]);
    let after = albums::find(&pool, alice.id, album).await.unwrap().unwrap();
    assert!(after.updated_at > before.updated_at);

    assert_eq!(
        albums::remove_items(&pool, album, Remover::AlbumOwner, &[a, bobs])
            .await
            .unwrap(),
        1
    );
    assert_eq!(item_names(&pool, alice.id, album).await, ["b.jpg"]);

    // Without a share Bob has no access (the API answers not_found), and as
    // a contributor he could only remove his own photos.
    assert!(access::album(&pool, bob.id, album).await.unwrap().is_none());
    assert_eq!(
        albums::remove_items(&pool, album, Remover::Contributor(bob.id), &[b])
            .await
            .unwrap(),
        0
    );
    assert!(albums::find(&pool, bob.id, album).await.unwrap().is_none());
    assert!(!albums::delete(&pool, album, bob.id).await.unwrap());
    assert_eq!(
        albums::update(&pool, album, bob.id, &AlbumUpdate::default())
            .await
            .unwrap(),
        UpdateOutcome::NotFound
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_album_item_names_its_photos_real_owner(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let bob_photos = root(&pool, bob.id, "Photos").await.id;
    let bobs = photo(
        &pool,
        bob.id,
        bob_photos,
        "bob.jpg",
        datetime!(2024-07-04 10:00),
    )
    .await;
    let album = albums::create(&pool, alice.id, &name("A"), &[])
        .await
        .unwrap();
    let insert = |owner: Uuid| {
        sqlx::query("INSERT INTO album_items (album_id, node_id, owner_id) VALUES ($1, $2, $3)")
            .bind(album)
            .bind(bobs)
            .bind(owner)
            .execute(&pool)
    };
    // Shared albums hold several people's photos, but never under a false owner.
    let err = insert(alice.id).await.unwrap_err();
    let constraint = match &err {
        sqlx::Error::Database(db) => db.constraint().map(str::to_owned),
        _ => None,
    };
    assert_eq!(constraint.as_deref(), Some("album_items_asset_fkey"));
    insert(bob.id).await.unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn covers_follow_choices_trash_and_deletion(pool: PgPool) {
    let alice = admin(&pool).await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    let old = photo(
        &pool,
        alice.id,
        photos,
        "old.jpg",
        datetime!(2023-01-01 10:00),
    )
    .await;
    let new = photo(
        &pool,
        alice.id,
        photos,
        "new.jpg",
        datetime!(2024-01-01 10:00),
    )
    .await;
    let outsider = photo(
        &pool,
        alice.id,
        photos,
        "out.jpg",
        datetime!(2024-02-01 10:00),
    )
    .await;
    let album = albums::create(&pool, alice.id, &name("A"), &[old, new])
        .await
        .unwrap();
    let cover = |id| AlbumUpdate {
        cover: CoverChange::Set(id),
        ..AlbumUpdate::default()
    };
    assert_eq!(
        albums::update(&pool, album, alice.id, &cover(outsider))
            .await
            .unwrap(),
        UpdateOutcome::CoverNotInAlbum
    );
    assert_eq!(
        albums::update(&pool, album, alice.id, &cover(old))
            .await
            .unwrap(),
        UpdateOutcome::Updated
    );
    let cover_id = |pool: PgPool| async move {
        albums::find(&pool, alice.id, album)
            .await
            .unwrap()
            .unwrap()
            .cover_id
    };
    assert_eq!(cover_id(pool.clone()).await, Some(old));

    // Clearing the choice shows the newest item; keeping leaves it alone.
    let clear = AlbumUpdate {
        cover: CoverChange::Clear,
        ..AlbumUpdate::default()
    };
    albums::update(&pool, album, alice.id, &clear)
        .await
        .unwrap();
    let found = albums::find(&pool, alice.id, album).await.unwrap().unwrap();
    assert_eq!((found.cover_node_id, found.cover_id), (None, Some(new)));
    albums::update(&pool, album, alice.id, &cover(old))
        .await
        .unwrap();
    albums::update(&pool, album, alice.id, &AlbumUpdate::default())
        .await
        .unwrap();
    assert_eq!(cover_id(pool.clone()).await, Some(old));

    // Only the owner's albums exist for them.
    let bob = invited(&pool, &alice, "bob").await;
    assert!(albums::exists(&pool, alice.id, album).await.unwrap());
    assert!(!albums::exists(&pool, bob.id, album).await.unwrap());
    assert!(
        !albums::exists(&pool, alice.id, Uuid::now_v7())
            .await
            .unwrap()
    );

    // A trashed cover is hidden (newest live item instead) and comes back on restore.
    raw_trash(&pool, old).await;
    assert_eq!(item_names(&pool, alice.id, album).await, ["new.jpg"]);
    assert_eq!(cover_id(pool.clone()).await, Some(new));
    raw_restore(&pool, old).await;
    assert_eq!(
        item_names(&pool, alice.id, album).await,
        ["new.jpg", "old.jpg"]
    );
    assert_eq!(cover_id(pool.clone()).await, Some(old));

    // Deleting the node for good clears it from albums and nulls the cover.
    sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
    let found = albums::find(&pool, alice.id, album).await.unwrap().unwrap();
    assert_eq!(
        (found.cover_node_id, found.item_count, found.cover_id),
        (None, 1, Some(new))
    );

    // Removing the chosen cover falls back to the newest item.
    albums::add_items(&pool, album, alice.id, &[outsider])
        .await
        .unwrap();
    albums::update(&pool, album, alice.id, &cover(new))
        .await
        .unwrap();
    albums::remove_items(&pool, album, Remover::AlbumOwner, &[new])
        .await
        .unwrap();
    let found = albums::find(&pool, alice.id, album).await.unwrap().unwrap();
    assert_eq!(
        (found.cover_node_id, found.cover_id),
        (None, Some(outsider))
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn items_page_like_the_timeline_and_memberships_are_listed(pool: PgPool) {
    let alice = admin(&pool).await;
    let photos = root(&pool, alice.id, "Photos").await.id;
    let mut ids = Vec::new();
    for day in 1..=5u8 {
        let at = time::PrimitiveDateTime::new(
            time::Date::from_calendar_date(2024, time::Month::July, day).unwrap(),
            time::Time::MIDNIGHT,
        );
        ids.push(photo(&pool, alice.id, photos, &format!("d{day}.jpg"), at).await);
    }
    let album = albums::create(&pool, alice.id, &name("B"), &ids)
        .await
        .unwrap();
    let other = albums::create(&pool, alice.id, &name("A"), &ids[..1])
        .await
        .unwrap();
    let first = albums::items(&pool, album, None, 2).await.unwrap();
    assert!(first.has_more);
    let names: Vec<&str> = first.items.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["d5.jpg", "d4.jpg"]);
    let cursor = first.items.last().unwrap().cursor();
    let rest = albums::items(&pool, album, Some(&cursor), 10)
        .await
        .unwrap();
    assert!(!rest.has_more);
    let names: Vec<&str> = rest.items.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["d3.jpg", "d2.jpg", "d1.jpg"]);

    let memberships = albums::albums_for_node(&pool, alice.id, ids[0])
        .await
        .unwrap();
    let listed: Vec<(Uuid, &str)> = memberships
        .iter()
        .map(|a| (a.id, a.name.as_str()))
        .collect();
    assert_eq!(listed, [(other, "A"), (album, "B")]);
    assert!(
        albums::albums_for_node(&pool, Uuid::now_v7(), ids[0])
            .await
            .unwrap()
            .is_empty()
    );
}
