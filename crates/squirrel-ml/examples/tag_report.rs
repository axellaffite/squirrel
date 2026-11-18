//! Tags every image of a directory with the committed label bank and prints
//! the five best labels with their probability and cosine, to calibrate the
//! thresholds in `src/clip/labels.rs` by eye:
//!
//!     SQUIRREL_ORT_DYLIB=… SQUIRREL_MODELS_DIR=… cargo run --release -p squirrel-ml \
//!       --features test-util --example tag_report -- <dir>

use squirrel_ml::clip::labels::{self, TOP_K};
use squirrel_ml::clip::{ClipImage, ClipText};
use squirrel_ml::input::{MAX_PREVIEW_PIXELS, load_preview};
use squirrel_ml::models::CLIP_MODEL_ID;
use squirrel_ml::testutil::test_ml;

fn main() -> anyhow::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: tag_report <dir>"))?;
    let (dylib, models) = test_ml()
        .ok_or_else(|| anyhow::anyhow!("set SQUIRREL_ORT_DYLIB and SQUIRREL_MODELS_DIR"))?;
    squirrel_ml::runtime::init(&dylib)?;
    let mut text = ClipText::load(&models, 4)?;
    let mut image = ClipImage::load(&models, 4)?;
    let bank = labels::compute_bank(&mut text, &labels::bank())?;
    let thresholds = labels::thresholds(CLIP_MODEL_ID).expect("thresholds for the pinned model");
    let mut files: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    for file in files {
        let Ok(picture) = load_preview(&file, MAX_PREVIEW_PIXELS) else {
            continue;
        };
        let scores = labels::score(&image.embed(&picture)?, &bank);
        let kept = labels::select(&scores, thresholds, TOP_K);
        println!("{}", file.display());
        for s in scores.iter().take(TOP_K) {
            let mark = if kept.iter().any(|(k, _)| *k == s.slug) {
                "*"
            } else {
                " "
            };
            println!(
                "  {mark} {:<20} p={:.3} cos={:.3}",
                s.slug, s.probability, s.cosine
            );
        }
    }
    Ok(())
}
