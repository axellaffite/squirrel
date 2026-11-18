//! Tags every image of a directory with the committed label bank and prints,
//! per photo, the best labels (cosine, margin over the background, kept or
//! not), then how many tags photos get and the share with none: to calibrate
//! the thresholds in `src/clip/labels.rs` by eye.
//!
//!     SQUIRREL_ORT_DYLIB=… SQUIRREL_MODELS_DIR=… cargo run --release -p squirrel-ml \
//!       --features test-util --example tag_report -- <dir>

use squirrel_ml::clip::labels::{self, TOP_K};
use squirrel_ml::clip::{ClipImage, ClipText};
use squirrel_ml::input::{MAX_PREVIEW_PIXELS, load_preview};
use squirrel_ml::models::{CLIP_MODEL_ID, ModelGroup};
use squirrel_ml::testutil::test_ml_for;

fn main() -> anyhow::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: tag_report <dir>"))?;
    let (dylib, models) = test_ml_for(&[ModelGroup::ClipImage, ModelGroup::ClipText])
        .ok_or_else(|| anyhow::anyhow!("set SQUIRREL_ORT_DYLIB and SQUIRREL_MODELS_DIR"))?;
    squirrel_ml::runtime::init(&dylib)?;
    let mut text = ClipText::load(&models, 4)?;
    let mut image = ClipImage::load(&models, 4)?;
    let label_list = labels::bank();
    let bank = labels::compute_bank(&mut text, &label_list)?;
    let thresholds = labels::thresholds(CLIP_MODEL_ID).expect("thresholds for the pinned model");
    println!("thresholds: {thresholds:?}");
    let mut files: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    let mut counts = Vec::new();
    for file in files {
        let Ok(picture) = load_preview(&file, MAX_PREVIEW_PIXELS) else {
            continue;
        };
        let scores = labels::score(&image.embed(&picture)?, &bank);
        let kept = labels::select(&scores, &label_list, thresholds, TOP_K);
        println!("{} ({} tags)", file.display(), kept.len());
        for s in scores.iter().take(TOP_K + 3) {
            let mark = if kept.iter().any(|(k, _)| *k == s.slug) {
                "*"
            } else {
                " "
            };
            println!(
                "  {mark} {:<20} cos={:.3} margin={:+.3}",
                s.slug, s.cosine, s.margin
            );
        }
        counts.push(kept.len());
    }
    if !counts.is_empty() {
        let untagged = counts.iter().filter(|c| **c == 0).count();
        let mut histogram = [0usize; TOP_K + 1];
        for count in &counts {
            histogram[*count] += 1;
        }
        println!(
            "{} photos: tags per photo {histogram:?} (index = count), {:.0} % without tags",
            counts.len(),
            100.0 * untagged as f64 / counts.len() as f64
        );
    }
    Ok(())
}
