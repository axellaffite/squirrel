use sqlx::PgPool;
use uuid::Uuid;

const INITIAL: &str = include_str!("../migrations/20261113000001_initial.sql");
const FILES: &str = include_str!("../migrations/20261113000002_files.sql");

async fn insert_user(pool: &PgPool, username: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (id, username, display_name, password_hash, role) \
         VALUES (gen_random_uuid(), $1, $1, 'x', 'user') RETURNING id",
    )
    .bind(username)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn root(pool: &PgPool, owner: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM nodes WHERE owner_id = $1 AND parent_id IS NULL AND name = $2",
    )
    .bind(owner)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_blob(pool: &PgPool, fill: char, size: i64) -> String {
    let sha = fill.to_string().repeat(64);
    sqlx::query("INSERT INTO blobs (sha256, size, mime) VALUES ($1, $2, 'text/plain')")
        .bind(&sha)
        .bind(size)
        .execute(pool)
        .await
        .unwrap();
    sha
}

async fn insert_node(
    pool: &PgPool,
    owner: Uuid,
    parent: Option<Uuid>,
    name: &str,
    blob: Option<(&str, i64)>,
) -> Result<Uuid, sqlx::Error> {
    let kind = if blob.is_some() { "file" } else { "folder" };
    sqlx::query_scalar(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime) \
         VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(owner)
    .bind(parent)
    .bind(name)
    .bind(kind)
    .bind(blob.map(|(sha, _)| sha.to_owned()))
    .bind(blob.map_or(0, |(_, size)| size))
    .bind(blob.map(|_| "text/plain"))
    .fetch_one(pool)
    .await
}

async fn ref_count(pool: &PgPool, sha: &str) -> (i64, bool) {
    sqlx::query_as("SELECT ref_count, zero_since IS NOT NULL FROM blobs WHERE sha256 = $1")
        .bind(sha)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn constraint_of(err: &sqlx::Error) -> Option<String> {
    match err {
        sqlx::Error::Database(db) => db.constraint().map(str::to_owned),
        _ => None,
    }
}

#[sqlx::test(migrations = false)]
async fn backfills_default_roots_for_existing_users(pool: PgPool) {
    sqlx::raw_sql(INITIAL).execute(&pool).await.unwrap();
    let alice = insert_user(&pool, "alice").await;
    sqlx::raw_sql(FILES).execute(&pool).await.unwrap();

    let names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM nodes WHERE owner_id = $1 AND parent_id IS NULL AND kind = 'folder' ORDER BY name",
    )
    .bind(alice)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(names, ["Documents", "Files", "Photos"]);
}

#[sqlx::test(migrations = false)]
async fn live_sibling_names_are_unique_including_roots(pool: PgPool) {
    sqlx::raw_sql(INITIAL).execute(&pool).await.unwrap();
    let alice = insert_user(&pool, "alice").await;
    sqlx::raw_sql(FILES).execute(&pool).await.unwrap();
    let files = root(&pool, alice, "Files").await;

    let err = insert_node(&pool, alice, None, "Files", None)
        .await
        .unwrap_err();
    assert_eq!(constraint_of(&err).as_deref(), Some("nodes_live_name_key"));

    let first = insert_node(&pool, alice, Some(files), "a", None)
        .await
        .unwrap();
    let err = insert_node(&pool, alice, Some(files), "a", None)
        .await
        .unwrap_err();
    assert_eq!(constraint_of(&err).as_deref(), Some("nodes_live_name_key"));

    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    insert_node(&pool, alice, Some(files), "a", None)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn shape_constraints(pool: PgPool) {
    let alice = insert_user(&pool, "alice").await;
    sqlx::query(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind) \
                 SELECT gen_random_uuid(), $1, NULL, n, 'folder' \
                 FROM unnest(ARRAY['Photos','Documents','Files']) AS n",
    )
    .bind(alice)
    .execute(&pool)
    .await
    .unwrap();
    let files = root(&pool, alice, "Files").await;
    let sha = insert_blob(&pool, 'a', 3).await;

    // A file needs a blob, a folder must not have one, roots are folders.
    let err = sqlx::query(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind) VALUES (gen_random_uuid(), $1, $2, 'x', 'file')",
    )
    .bind(alice)
    .bind(files)
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(constraint_of(&err).as_deref(), Some("nodes_file_has_blob"));
    let err = insert_node(&pool, alice, None, "loose.txt", Some((&sha, 3)))
        .await
        .unwrap_err();
    assert_eq!(
        constraint_of(&err).as_deref(),
        Some("nodes_roots_are_folders")
    );
    assert!(
        insert_node(&pool, alice, Some(files), &"a".repeat(256), None)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("INSERT INTO blobs (sha256, size, mime) VALUES ('../etc', 1, 'x')")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn ref_count_follows_file_nodes_including_cascades(pool: PgPool) {
    let alice = insert_user(&pool, "alice").await;
    let parent = insert_node(&pool, alice, None, "Files", None)
        .await
        .unwrap();
    let sha = insert_blob(&pool, 'b', 5).await;
    assert_eq!(ref_count(&pool, &sha).await, (0, true));

    let folder = insert_node(&pool, alice, Some(parent), "dir", None)
        .await
        .unwrap();
    insert_node(&pool, alice, Some(parent), "one.txt", Some((&sha, 5)))
        .await
        .unwrap();
    insert_node(&pool, alice, Some(folder), "two.txt", Some((&sha, 5)))
        .await
        .unwrap();
    assert_eq!(ref_count(&pool, &sha).await, (2, false));

    sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(folder)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(ref_count(&pool, &sha).await, (1, false));
    sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(parent)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(ref_count(&pool, &sha).await, (0, true));
}

async fn insert_session(
    pool: &PgPool,
    owner: Uuid,
    parent: Uuid,
    size: i64,
    chunk_size: i64,
    chunk_count: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO upload_sessions \
           (id, owner_id, created_by, parent_id, name, sha256, size, chunk_size, chunk_count, \
            expires_at) \
         VALUES (gen_random_uuid(), $1, $1, $2, 'f', repeat('a', 64), $3, $4, $5, \
                 now() + interval '1 hour')",
    )
    .bind(owner)
    .bind(parent)
    .bind(size)
    .bind(chunk_size)
    .bind(chunk_count)
    .execute(pool)
    .await
    .map(|_| ())
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn parents_belong_to_the_same_owner(pool: PgPool) {
    let alice = insert_user(&pool, "alice").await;
    let bob = insert_user(&pool, "bob").await;
    let alices_root = insert_node(&pool, alice, None, "Files", None)
        .await
        .unwrap();
    insert_node(&pool, bob, None, "Files", None).await.unwrap();

    let err = insert_node(&pool, bob, Some(alices_root), "intruder", None)
        .await
        .unwrap_err();
    assert_eq!(constraint_of(&err).as_deref(), Some("nodes_parent_fkey"));
    insert_node(&pool, alice, Some(alices_root), "mine", None)
        .await
        .unwrap();

    let err = insert_session(&pool, bob, alices_root, 1, 1, 1)
        .await
        .unwrap_err();
    assert_eq!(
        constraint_of(&err).as_deref(),
        Some("upload_sessions_parent_fkey")
    );
    insert_session(&pool, alice, alices_root, 1, 1, 1)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn upload_sessions_have_a_consistent_chunk_count(pool: PgPool) {
    let alice = insert_user(&pool, "alice").await;
    let root = insert_node(&pool, alice, None, "Files", None)
        .await
        .unwrap();
    for (size, chunk_size, count) in [
        (0, 4, 0),
        (8, 4, 2),
        (9, 4, 3),
        (10_000_000_000, 1 << 23, 1193),
    ] {
        insert_session(&pool, alice, root, size, chunk_size, count)
            .await
            .unwrap_or_else(|e| panic!("{size}/{chunk_size}: {e}"));
    }
    for (size, chunk_size, count) in [(0, 4, 1), (8, 4, 3), (9, 4, 2), (1, 4, 0)] {
        let err = insert_session(&pool, alice, root, size, chunk_size, count)
            .await
            .unwrap_err();
        assert_eq!(
            constraint_of(&err).as_deref(),
            Some("upload_sessions_chunk_geometry"),
            "{size}/{chunk_size}/{count}"
        );
    }
}
