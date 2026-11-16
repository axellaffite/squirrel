use std::time::Duration;

use squirrel_media::exec::{ExecRequest, ExecResponse, TEXT_FILE};
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
