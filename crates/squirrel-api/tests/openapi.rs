#[test]
fn committed_openapi_json_is_up_to_date() {
    let generated = squirrel_api::openapi()
        .to_pretty_json()
        .expect("serializable spec");
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../openapi.json");
    let committed = std::fs::read_to_string(path).unwrap_or_default();
    assert_eq!(
        committed.trim(),
        generated.trim(),
        "openapi.json is stale — run: cargo run -q -p squirrel -- openapi > openapi.json"
    );
}
