mod common;

use common::{admin, invited, raw_file, raw_trash, root};
use sqlx::PgPool;
use squirrel_core::MediaKind;
use squirrel_db::documents::{self, SearchHit};
use squirrel_db::media::{self, MediaState};
use uuid::Uuid;

/// A text file node of blob `fill` with a media row, and its text stored.
async fn document(
    pool: &PgPool,
    owner: Uuid,
    parent: Uuid,
    name: &str,
    fill: char,
    text: Option<&str>,
) -> Uuid {
    let id = raw_file(pool, owner, parent, name, fill, 10).await;
    let sha = common::hash(fill);
    let mut conn = pool.acquire().await.unwrap();
    media::ensure(&mut conn, sha.as_str(), MediaKind::Text)
        .await
        .unwrap();
    drop(conn);
    if let Some(text) = text {
        assert!(
            documents::store_text(pool, sha.as_str(), text, None)
                .await
                .unwrap()
        );
    }
    id
}

async fn search(pool: &PgPool, owner: Uuid, query: &str) -> Vec<SearchHit> {
    documents::search(pool, owner, query, 0, 20)
        .await
        .unwrap()
        .items
}

fn ids(hits: &[SearchHit]) -> Vec<Uuid> {
    hits.iter().map(|hit| hit.node.id).collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn stores_and_replaces_text_and_marks_it_ready(pool: PgPool) {
    let alice = admin(&pool).await;
    let docs = root(&pool, alice.id, "Documents").await;
    let id = document(&pool, alice.id, docs.id, "n.txt", 'a', None).await;
    let sha = common::hash('a');
    let before = media::find(&pool, sha.as_str()).await.unwrap().unwrap();
    assert_eq!(before.text_state(), MediaState::Pending);

    assert!(
        documents::store_text(&pool, sha.as_str(), "Quarterly squirrel\0 budget", Some(4))
            .await
            .unwrap()
    );
    let after = media::find(&pool, sha.as_str()).await.unwrap().unwrap();
    assert_eq!(after.text_state(), MediaState::Ready);
    assert_eq!(after.page_count, Some(4), "recorded with the text");
    assert_eq!(ids(&search(&pool, alice.id, "budget").await), vec![id]);

    documents::store_text(&pool, sha.as_str(), "Acorn inventory", None)
        .await
        .unwrap();
    assert!(search(&pool, alice.id, "budget").await.is_empty());
    assert_eq!(ids(&search(&pool, alice.id, "acorn").await), vec![id]);
    let body: String = sqlx::query_scalar("SELECT body FROM document_text WHERE sha256 = $1")
        .bind(sha.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(body, "Acorn inventory");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn text_of_a_collected_blob_is_not_stored(pool: PgPool) {
    let stored = documents::store_text(&pool, &"f".repeat(64), "orphan", None)
        .await
        .unwrap();
    assert!(!stored);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn finds_by_folded_prefixes_in_text_and_names(pool: PgPool) {
    let alice = admin(&pool).await;
    let docs = root(&pool, alice.id, "Documents").await;
    let invoice = document(
        &pool,
        alice.id,
        docs.id,
        "invoice.txt",
        'a',
        Some("Facture École Électricité 2026, payée le 2026-01-15."),
    )
    .await;
    let named = document(&pool, alice.id, docs.id, "ecole-inscription.pdf", 'b', None).await;
    document(&pool, alice.id, docs.id, "other.txt", 'c', Some("nothing")).await;

    let hits = search(&pool, alice.id, "ecole elec").await;
    assert_eq!(ids(&hits), vec![invoice]);
    let snippet = hits[0].snippet.as_deref().unwrap();
    assert!(snippet.contains("\u{2}École\u{3}"), "{snippet:?}");
    assert!(snippet.contains("\u{2}Électricité\u{3}"), "{snippet:?}");

    // A name match ranks above a text-only match.
    let hits = search(&pool, alice.id, "ÉCOLE").await;
    assert_eq!(ids(&hits), vec![named, invoice]);
    assert!(hits[0].snippet.is_none());

    // Dates and file name parts are words of their own.
    assert_eq!(ids(&search(&pool, alice.id, "01").await), vec![invoice]);
    assert_eq!(ids(&search(&pool, alice.id, "inscr").await), vec![named]);
    assert!(search(&pool, alice.id, "ecole zebra").await.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn queries_without_usable_words_or_with_syntax_are_harmless(pool: PgPool) {
    let alice = admin(&pool).await;
    let docs = root(&pool, alice.id, "Documents").await;
    let id = document(&pool, alice.id, docs.id, "o.txt", 'a', Some("it's ok")).await;
    for nothing in ["", "' & | !", "a"] {
        let page = documents::search(&pool, alice.id, nothing, 0, 20)
            .await
            .unwrap();
        assert!(page.items.is_empty() && !page.has_more, "{nothing:?}");
    }
    for hostile in ["ok'", "ok\\", "ok:*", "!(ok | x)", "ok <-> it", "'ok'''"] {
        assert_eq!(
            ids(&search(&pool, alice.id, hostile).await),
            vec![id],
            "{hostile:?}"
        );
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_the_owners_live_files_are_found(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let alice_docs = root(&pool, alice.id, "Documents").await;
    let bob_docs = root(&pool, bob.id, "Documents").await;
    // The same content (one blob) in both libraries, and twice in Bob's.
    let mine = document(
        &pool,
        alice.id,
        alice_docs.id,
        "a.txt",
        'a',
        Some("secret plan"),
    )
    .await;
    let bobs = raw_file(&pool, bob.id, bob_docs.id, "b.txt", 'a', 10).await;
    let trashed = raw_file(&pool, bob.id, bob_docs.id, "c.txt", 'a', 10).await;
    raw_trash(&pool, trashed).await;
    let named = raw_file(&pool, bob.id, bob_docs.id, "secret.txt", 'd', 10).await;
    raw_trash(&pool, named).await;

    assert_eq!(ids(&search(&pool, alice.id, "secret").await), vec![mine]);
    let hits = search(&pool, bob.id, "secret plan").await;
    assert_eq!(ids(&hits), vec![bobs], "once, and not the trashed copy");
    assert!(hits.iter().all(|hit| hit.node.owner_id == bob.id));

    raw_trash(&pool, mine).await;
    assert!(search(&pool, alice.id, "secret").await.is_empty());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn pages_with_offset_and_limit(pool: PgPool) {
    let alice = admin(&pool).await;
    let docs = root(&pool, alice.id, "Documents").await;
    let mut all = Vec::new();
    for (i, fill) in ['a', 'b', 'c', 'd', 'e'].into_iter().enumerate() {
        all.push(
            document(
                &pool,
                alice.id,
                docs.id,
                &format!("report-{i}.txt"),
                fill,
                None,
            )
            .await,
        );
    }
    let mut seen = Vec::new();
    let mut offset = 0;
    loop {
        let page = documents::search(&pool, alice.id, "report", offset, 2)
            .await
            .unwrap();
        assert!(page.items.len() <= 2);
        seen.extend(ids(&page.items));
        if !page.has_more {
            break;
        }
        offset += 2;
    }
    seen.sort();
    all.sort();
    assert_eq!(seen, all);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn short_words_match_whole_words_only(pool: PgPool) {
    let alice = admin(&pool).await;
    let docs = root(&pool, alice.id, "Documents").await;
    let id = document(
        &pool,
        alice.id,
        docs.id,
        "n.txt",
        'a',
        Some("electricity in EU"),
    )
    .await;
    assert!(
        search(&pool, alice.id, "el").await.is_empty(),
        "no 2-letter prefix"
    );
    assert_eq!(ids(&search(&pool, alice.id, "eu").await), vec![id]);
    assert_eq!(ids(&search(&pool, alice.id, "ele").await), vec![id]);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn words_joined_by_dots_are_highlighted(pool: PgPool) {
    let alice = admin(&pool).await;
    let docs = root(&pool, alice.id, "Documents").await;
    document(
        &pool,
        alice.id,
        docs.id,
        "n.txt",
        'a',
        Some("See Rapport.pdf attached"),
    )
    .await;
    let hits = search(&pool, alice.id, "rapport").await;
    let snippet = hits[0].snippet.as_deref().unwrap();
    assert!(snippet.contains("\u{2}Rapport\u{3}"), "{snippet:?}");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn name_matches_come_first_whatever_the_text_rank(pool: PgPool) {
    let alice = admin(&pool).await;
    let docs = root(&pool, alice.id, "Documents").await;
    let dense = "acorn ".repeat(500);
    let text = document(&pool, alice.id, docs.id, "notes.txt", 'a', Some(&dense)).await;
    let named = document(&pool, alice.id, docs.id, "acorns.txt", 'b', Some("nothing")).await;
    assert_eq!(
        ids(&search(&pool, alice.id, "acorn").await),
        vec![named, text]
    );
}
