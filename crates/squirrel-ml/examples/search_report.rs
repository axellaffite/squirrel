//! Scores search queries against every image of a directory as photo search
//! does: per query, the closest label (text cosine) and its rival siblings;
//! per photo, p(query), the best rival's p, the logit lead, and whether the
//! photo would be kept. To calibrate the search constants in `src/clip/mod.rs`.
//!
//!     SQUIRREL_ORT_DYLIB=… SQUIRREL_MODELS_DIR=… cargo run --release -p squirrel-ml \
//!       --features test-util --example search_report -- <dir> <query>…

use squirrel_ml::clip::{self, ClipImage, ClipText, LOGIT_BIAS, LOGIT_SCALE, cosine, labels};
use squirrel_ml::input::{MAX_PREVIEW_PIXELS, load_preview};
use squirrel_ml::models::ModelGroup;
use squirrel_ml::testutil::test_ml_for;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: search_report <dir> <query>…"))?;
    let queries: Vec<String> = args.collect();
    let (dylib, models) = test_ml_for(&[ModelGroup::ClipImage, ModelGroup::ClipText])
        .ok_or_else(|| anyhow::anyhow!("set SQUIRREL_ORT_DYLIB and SQUIRREL_MODELS_DIR"))?;
    squirrel_ml::runtime::init(&dylib)?;
    let mut text = ClipText::load(&models, 4)?;
    let mut image = ClipImage::load(&models, 4)?;
    let label_list = labels::bank();
    let bank = labels::compute_bank(&mut text, &label_list)?;
    let mut files: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    let mut photos = Vec::new();
    for file in files {
        if let Ok(picture) = load_preview(&file, MAX_PREVIEW_PIXELS) {
            let name = file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            photos.push((name, image.embed(&picture)?));
        }
    }
    let logit = |cos: f32| LOGIT_SCALE * cos + LOGIT_BIAS;
    for query in &queries {
        let embedding = text.embed(query)?;
        let mut near: Vec<(&str, f32)> = bank
            .iter()
            .map(|(slug, t)| (slug.as_str(), cosine(&embedding, t)))
            .collect();
        near.sort_by(|a, b| b.1.total_cmp(&a.1));
        let rivals = labels::search_rivals(&embedding, &bank, &label_list, clip::QUERY_LABEL_MATCH);
        let rival_names: Vec<&str> = rivals.iter().map(|(s, _)| *s).collect();
        println!(
            "{query:?}: closest {:?}, rivals {rival_names:?}",
            &near[..3.min(near.len())]
        );
        for (name, photo) in &photos {
            let q = cosine(photo, &embedding);
            let best_rival = rivals
                .iter()
                .map(|(slug, t)| (*slug, cosine(photo, t)))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            let lead = best_rival.map(|(_, r)| logit(q) - logit(r));
            let kept = q >= clip::search_min_cosine()
                && lead.is_none_or(|lead| lead >= clip::SEARCH_CONTRAST);
            println!(
                "  {} {name:<24} p={:.4} logit={:+6.2} rival={:?} lead={:?}",
                if kept { "*" } else { " " },
                clip::match_probability(q),
                logit(q),
                best_rival.map(|(s, r)| (s, (clip::match_probability(r) * 1e4).round() / 1e4)),
                lead.map(|l| (l * 100.0).round() / 100.0),
            );
        }
    }
    Ok(())
}
