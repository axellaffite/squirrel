//! Times libvips previews per size bucket, in-process and in a fresh child
//! process (spawn + `vips_init` + render), for generated 12 MP / 50 MP JPEGs
//! and, when libheif's `heif-enc` is on the PATH, 12 MP / 48 MP HEICs.
//!
//! ```text
//! cargo run --release -p squirrel-media --features vips --example preview_bench
//! ```
//!
//! Peak memory of one render: run a child under the system's `time`, e.g.
//! `/usr/bin/time -l target/release/examples/preview_bench --child in.jpg 3200 jpeg`
//! (macOS; `/usr/bin/time -v` on Linux). The child also prints libvips' own
//! pixel-buffer high-water mark.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use image::{ImageBuffer, Rgb};
use libvips::{VipsApp, VipsImage, ops};

const BUCKETS: [i32; 3] = [1440, 2160, 3200];
const RUNS: usize = 5;
const CHILD_RUNS: usize = 3;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--child") {
        child(&args[2..]);
        return;
    }
    // `PREVIEW_BENCH_DIR`: keep the generated inputs there (to rerun children by hand).
    let temp = tempfile::tempdir().expect("temp dir");
    let dir =
        std::env::var_os("PREVIEW_BENCH_DIR").map_or_else(|| temp.path().to_owned(), PathBuf::from);
    let inputs = inputs(&dir);
    let app = start();
    println!("libvips {}", app.version_string().unwrap_or("?"));
    println!(
        "spawn + vips_init alone: {} ms",
        median_spawn(None).as_millis()
    );
    println!("input             bucket  format  in-process  child");
    for (name, path) in &inputs {
        for bucket in BUCKETS {
            for format in ["jpeg", "webp"] {
                let local = median((0..RUNS).map(|_| time(|| render(path, bucket, format))));
                let spawned = median_spawn(Some((path, bucket, format)));
                println!(
                    "{name:<17} {bucket:>6}  {format:<6}  {:>8} ms  {} ms",
                    local.as_millis(),
                    spawned.as_millis()
                );
            }
        }
    }
}

/// The sandbox's libvips settings: no operation cache, two threads, no
/// untrusted loaders.
fn start() -> VipsApp {
    let app = VipsApp::new("preview_bench", false).expect("vips_init");
    app.cache_set_max(0);
    app.cache_set_max_mem(0);
    app.concurrency_set(2);
    // SAFETY: a plain setter on an initialised libvips.
    unsafe { libvips::bindings::vips_block_untrusted_set(1) };
    app
}

fn render(path: &Path, bucket: i32, format: &str) -> usize {
    let options = ops::ThumbnailOptions {
        height: bucket,
        size: ops::Size::Down,
        fail_on: ops::FailOn::Error,
        ..ops::ThumbnailOptions::default()
    };
    let image: VipsImage =
        ops::thumbnail_with_opts(path.to_str().expect("utf-8 path"), bucket, &options)
            .expect("thumbnail");
    let bytes = match format {
        "jpeg" => ops::jpegsave_buffer_with_opts(
            &image,
            &ops::JpegsaveBufferOptions {
                q: 85,
                keep: ops::ForeignKeep::None,
                ..ops::JpegsaveBufferOptions::default()
            },
        ),
        _ => ops::webpsave_buffer_with_opts(
            &image,
            &ops::WebpsaveBufferOptions {
                q: 80,
                keep: ops::ForeignKeep::None,
                ..ops::WebpsaveBufferOptions::default()
            },
        ),
    };
    bytes.expect("encode").len()
}

/// `--child [<input> <bucket> <format>]`: one render (or none) in a fresh process.
fn child(args: &[String]) {
    let app = start();
    if let [input, bucket, format] = args {
        let bucket: i32 = bucket.parse().expect("bucket");
        let bytes = render(Path::new(input), bucket, format);
        eprintln!(
            "{bytes} bytes, libvips high-water {} MiB",
            app.tracked_get_mem_highwater() / (1024 * 1024)
        );
    }
}

fn median_spawn(job: Option<(&PathBuf, i32, &str)>) -> Duration {
    let exe = std::env::current_exe().expect("own path");
    median((0..CHILD_RUNS).map(|_| {
        time(|| {
            let mut command = Command::new(&exe);
            command.arg("--child");
            if let Some((path, bucket, format)) = job {
                command.arg(path).arg(bucket.to_string()).arg(format);
            }
            let output = command.output().expect("spawn");
            assert!(output.status.success(), "{output:?}");
        })
    }))
}

fn time<T>(work: impl FnOnce() -> T) -> Duration {
    let start = Instant::now();
    std::hint::black_box(work());
    start.elapsed()
}

fn median(samples: impl Iterator<Item = Duration>) -> Duration {
    let mut samples: Vec<Duration> = samples.collect();
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// Photo-like inputs: gradients plus noise, so the encoders work as hard as on
/// real pictures.
fn inputs(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    for (name, w, h) in [("12mp", 4000, 3000), ("50mp", 8160, 6120)] {
        let path = dir.join(format!("{name}.jpg"));
        photo(w, h).save(&path).expect("jpeg");
        found.push((format!("{name}.jpg"), path));
    }
    for (name, w, h) in [("12mp", 4032, 3024), ("48mp", 8064, 6048)] {
        let jpeg = dir.join(format!("{name}-src.jpg"));
        photo(w, h).save(&jpeg).expect("jpeg");
        let heic = dir.join(format!("{name}.heic"));
        let encoded = Command::new("heif-enc")
            .args(["-q", "50", "-o"])
            .arg(&heic)
            .arg(&jpeg)
            .output();
        match encoded {
            Ok(out) if out.status.success() => found.push((format!("{name}.heic"), heic)),
            _ => eprintln!("heif-enc unavailable: skipping {name}.heic"),
        }
    }
    found
}

fn photo(width: u32, height: u32) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let mut seed = 0x2545_f491_u32;
    ImageBuffer::from_fn(width, height, |x, y| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let noise = (seed & 0x1f) as u8;
        Rgb([
            ((x * 255 / width) as u8).saturating_add(noise),
            ((y * 255 / height) as u8).saturating_add(noise / 2),
            (((x + y) / 16) as u8).wrapping_add(noise),
        ])
    })
}
