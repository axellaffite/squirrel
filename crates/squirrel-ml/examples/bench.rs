//! Per-stage CPU timings of the image path (SigLIP2), on generated 1440 × 960
//! previews (or the files of `--dir`), or with `--texts N` the text tower's
//! latency on N search queries:
//!
//!     SQUIRREL_ORT_DYLIB=… SQUIRREL_MODELS_DIR=… cargo run --release -p squirrel-ml \
//!       --features test-util --example bench -- --images 200 --threads 2 [--dir previews/]
//!     … --example bench -- --texts 50 --threads 2
//!
//! Peak RSS: run it under `/usr/bin/time -l` (macOS) or `/usr/bin/time -v` (Linux).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use squirrel_ml::clip::image::PREPROCESS_CFG;
use squirrel_ml::clip::preprocess::{PreprocessCfg, preprocess};
use squirrel_ml::clip::{ClipImage, ClipText};
use squirrel_ml::input::{MAX_PREVIEW_PIXELS, load_preview};
use squirrel_ml::models::ModelGroup;
use squirrel_ml::testutil::{Shape, shape_image, test_ml_for};

struct Args {
    images: usize,
    texts: usize,
    threads: usize,
    dir: Option<PathBuf>,
}

fn args() -> anyhow::Result<Args> {
    let mut args = Args {
        images: 200,
        texts: 0,
        threads: 2,
        dir: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))?;
        match flag.as_str() {
            "--images" => args.images = value.parse()?,
            "--texts" => args.texts = value.parse()?,
            "--threads" => args.threads = value.parse()?,
            "--dir" => args.dir = Some(value.into()),
            other => anyhow::bail!("unknown option {other}"),
        }
    }
    Ok(args)
}

/// Preview files: those of `--dir`, or generated JPEGs in a temporary directory.
fn inputs(args: &Args, scratch: &std::path::Path) -> anyhow::Result<Vec<PathBuf>> {
    if let Some(dir) = &args.dir {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.is_file())
            .take(args.images)
            .collect();
        files.sort();
        return Ok(files);
    }
    (0..args.images)
        .map(|n| {
            let shape = Shape::ALL[n % Shape::ALL.len()];
            let square = shape_image(shape, 960);
            let image = image::RgbImage::from_fn(1440, 960, |x, y| {
                let noise = ((x * 7 + y * 13 + n as u32 * 31) % 23) as u8;
                let base = if (240..1200).contains(&x) {
                    square.get_pixel(x - 240, y).0
                } else {
                    [200, 200, 200]
                };
                image::Rgb(base.map(|c| c.saturating_sub(noise)))
            });
            let path = scratch.join(format!("{n:04}.jpg"));
            image.save(&path)?;
            Ok(path)
        })
        .collect()
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// Loads the text tower alone, then times `count` short queries.
fn texts(args: &Args) -> anyhow::Result<()> {
    let (dylib, models) = test_ml_for(&[ModelGroup::ClipText])
        .ok_or_else(|| anyhow::anyhow!("set SQUIRREL_ORT_DYLIB and SQUIRREL_MODELS_DIR"))?;
    let started = Instant::now();
    models.verify(ModelGroup::ClipText)?;
    let verified = started.elapsed();
    let version = squirrel_ml::runtime::init(&dylib)?;
    let mut text = ClipText::load(&models, args.threads)?;
    let loaded = started.elapsed();
    let queries = [
        "dog on the beach",
        "chien",
        "gâteau d'anniversaire",
        "snowy mountains",
    ];
    let mut times = Vec::new();
    for n in 0..args.texts {
        let t0 = Instant::now();
        text.embed(&format!("{} {n}", queries[n % queries.len()]))?;
        times.push(t0.elapsed());
    }
    println!(
        "ONNX Runtime {version}, {} queries, {} threads; verify {:.2?}, load {:.2?}; \
         median per query {:.1?}",
        args.texts,
        args.threads,
        verified,
        loaded - verified,
        median(times)
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = args()?;
    if args.texts > 0 {
        return texts(&args);
    }
    let (dylib, models) = test_ml_for(&[ModelGroup::ClipImage])
        .ok_or_else(|| anyhow::anyhow!("set SQUIRREL_ORT_DYLIB and SQUIRREL_MODELS_DIR"))?;
    let scratch = tempfile::tempdir()?;
    let files = inputs(&args, scratch.path())?;
    anyhow::ensure!(!files.is_empty(), "no input images");
    let started = Instant::now();
    models.verify(ModelGroup::ClipImage)?;
    let verified = started.elapsed();
    let version = squirrel_ml::runtime::init(&dylib)?;
    let mut clip = ClipImage::load(&models, args.threads)?;
    let loaded = started.elapsed();
    let cfg = PreprocessCfg::parse(&std::fs::read(models.file(PREPROCESS_CFG))?)?;
    let (mut decode, mut prep, mut encode) = (Vec::new(), Vec::new(), Vec::new());
    for file in &files {
        let t0 = Instant::now();
        let image = load_preview(file, MAX_PREVIEW_PIXELS)?;
        let t1 = Instant::now();
        let _tensor = preprocess(&image, &cfg)?;
        let t2 = Instant::now();
        clip.embed(&image)?;
        let t3 = Instant::now();
        decode.push(t1 - t0);
        prep.push(t2 - t1);
        // `embed` preprocesses again: count only the encoder.
        encode.push((t3 - t2).saturating_sub(t2 - t1));
    }
    let total = decode.iter().chain(&prep).chain(&encode).sum::<Duration>();
    println!(
        "ONNX Runtime {version}, {} images, {} threads; verify {:.2?}, load {:.2?}",
        files.len(),
        args.threads,
        verified,
        loaded - verified
    );
    println!(
        "median per image: decode {:.1?}, preprocess {:.1?}, image encoder {:.1?}; \
         mean total {:.1?}",
        median(decode),
        median(prep),
        median(encode),
        total / u32::try_from(files.len())?
    );
    Ok(())
}
