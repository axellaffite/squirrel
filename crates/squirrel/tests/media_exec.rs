use std::time::Duration;

use squirrel_media::exec::{ExecRequest, ExecResponse, TEXT_FILE};
use squirrel_media::preview::{PreviewEncoding, PreviewEngine};
use squirrel_media::testutil::{ExifSpec, jpeg_sized_with_exif, pdf_with_text};
use squirrel_media::thumbs::DecodeLimits;
use squirrel_worker::exec::{ExecError, Executor, SubprocessConfig};

fn executor(timeout: Duration) -> Executor {
    Executor::Subprocess(SubprocessConfig {
        program: env!("CARGO_BIN_EXE_squirrel").into(),
        max_memory_mb: 2048,
        max_cpu_secs: 60,
        timeout,
    })
}

#[tokio::test]
async fn renders_in_a_child_process() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.jpg");
    std::fs::write(&input, jpeg_sized_with_exif(400, 300, &ExifSpec::default())).unwrap();
    let out = tempfile::tempdir().unwrap();
    let request = ExecRequest::Image {
        input,
        out_dir: out.path().into(),
        limits: DecodeLimits::default(),
    };
    match executor(Duration::from_secs(60))
        .run(request)
        .await
        .unwrap()
    {
        ExecResponse::Image { rendered } => {
            assert_eq!((rendered.width, rendered.height), (400, 300))
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(out.path().join("thumb.webp").is_file());
}

/// A phone-sized preview of a 4 MP photo, in the child with its preview
/// limits (RLIMIT_AS 1.5 GiB on Linux), through libvips when built with it.
#[tokio::test]
async fn renders_a_preview_in_a_child_process() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.jpg");
    let spec = ExifSpec {
        orientation: Some(6),
        ..ExifSpec::default()
    };
    std::fs::write(&input, jpeg_sized_with_exif(2400, 1800, &spec)).unwrap();
    let request = ExecRequest::Preview {
        input,
        out_dir: dir.path().into(),
        bucket: 2160,
        encoding: PreviewEncoding::Display,
        engine: PreviewEngine::Vips,
        limits: DecodeLimits::default(),
    };
    let response = executor(Duration::from_secs(60))
        .run(request)
        .await
        .unwrap();
    let ExecResponse::Preview { preview } = response else {
        panic!("unexpected {response:?}");
    };
    assert_eq!((preview.width, preview.height), (1620, 2160));
    let engine = if squirrel_media::VIPS_COMPILED {
        PreviewEngine::Vips
    } else {
        PreviewEngine::Builtin
    };
    assert_eq!(preview.engine, engine);
    assert!(dir.path().join("preview.jpg").is_file());
}

#[test]
fn the_probe_flag_reports_libvips() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_squirrel"))
        .args(["media-exec", "--probe"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let response: ExecResponse = serde_json::from_slice(&output.stdout).unwrap();
    let ExecResponse::Probe { report } = response else {
        panic!("unexpected {response:?}");
    };
    assert_eq!(report.compiled, squirrel_media::VIPS_COMPILED);
    assert_eq!(report.usable(), squirrel_media::VIPS_COMPILED, "{report:?}");
}

/// With pdfium (`scripts/fetch-pdfium.sh`); skipped otherwise.
#[tokio::test]
async fn renders_a_pdf_in_a_child_process() {
    let Some(pdfium_dir) = squirrel_media::testutil::test_pdfium_dir() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.pdf");
    std::fs::write(&input, pdf_with_text("Sandboxed acorns")).unwrap();
    let out = tempfile::tempdir().unwrap();
    let request = ExecRequest::Pdf {
        input,
        out_dir: out.path().into(),
        pdfium_dir,
        render: true,
        text: true,
    };
    match executor(Duration::from_secs(60))
        .run(request)
        .await
        .unwrap()
    {
        ExecResponse::Pdf {
            rendered: Some(rendered),
            page_count: 1,
        } => assert_eq!(rendered.height, 1440),
        other => panic!("unexpected {other:?}"),
    }
    let text = std::fs::read_to_string(out.path().join(TEXT_FILE)).unwrap();
    assert!(text.contains("Sandboxed acorns"), "{text}");
}

#[tokio::test]
async fn a_child_that_overruns_its_time_is_killed() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.jpg");
    std::fs::write(
        &input,
        jpeg_sized_with_exif(4000, 3000, &ExifSpec::default()),
    )
    .unwrap();
    let request = ExecRequest::Image {
        input,
        out_dir: dir.path().into(),
        limits: DecodeLimits::default(),
    };
    let err = executor(Duration::from_millis(1))
        .run(request)
        .await
        .unwrap_err();
    assert!(matches!(err, ExecError::TimedOut(_)), "{err}");
}

/// RLIMIT_AS is enforced on Linux only (macOS refuses it).
#[cfg(target_os = "linux")]
#[tokio::test]
async fn the_memory_limit_stops_a_large_decode() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.jpg");
    std::fs::write(
        &input,
        jpeg_sized_with_exif(4000, 3000, &ExifSpec::default()),
    )
    .unwrap();
    let executor = Executor::Subprocess(SubprocessConfig {
        program: env!("CARGO_BIN_EXE_squirrel").into(),
        max_memory_mb: 64,
        max_cpu_secs: 60,
        timeout: Duration::from_secs(60),
    });
    let request = ExecRequest::Image {
        input,
        out_dir: dir.path().into(),
        limits: DecodeLimits::default(),
    };
    let outcome = executor.run(request).await;
    assert!(
        !matches!(outcome, Ok(ExecResponse::Image { .. })),
        "{outcome:?}"
    );
}

/// With `heic`: the sandbox child registers the HEIC decoder itself (a real HEIC from
/// `rotated.heic`, generated in CI; a failure there when missing).
#[cfg(feature = "heic")]
#[tokio::test]
async fn decodes_heic_in_a_child_process() {
    let Some(heic) =
        squirrel_media::testutil::ci_fixture("../squirrel-media/tests/fixtures/rotated.heic")
    else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.heic");
    std::fs::write(&input, heic).unwrap();
    let request = ExecRequest::Image {
        input,
        out_dir: dir.path().into(),
        limits: DecodeLimits::default(),
    };
    match executor(Duration::from_secs(60))
        .run(request)
        .await
        .unwrap()
    {
        ExecResponse::Image { rendered } => {
            assert_eq!((rendered.width, rendered.height), (32, 64))
        }
        other => panic!("unexpected {other:?}"),
    }
}
