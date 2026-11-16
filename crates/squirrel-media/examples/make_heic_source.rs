//! Writes `tests/fixtures/rotated-source.jpg`, the JPEG CI encodes into `rotated.heic`
//! (`heif-enc -q 50`): 64 × 32 with EXIF orientation 6.
//!
//! cargo run -p squirrel-media --example make_heic_source --features test-util

fn main() -> std::io::Result<()> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/rotated-source.jpg"
    );
    std::fs::write(path, squirrel_media::testutil::rotated_source_jpeg())?;
    println!("wrote {path}");
    Ok(())
}
