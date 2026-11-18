//! `squirrel ml-exec` driven by the worker's supervisor, as in production.

use std::ffi::OsString;
use std::time::Duration;

use squirrel_ml::protocol::{MlBody, MlOp, MlOutput};
use squirrel_worker::ml_client::{MlBackend, MlChildConfig, MlClient};

fn config(args: &[&std::ffi::OsStr]) -> MlChildConfig {
    MlChildConfig {
        program: env!("CARGO_BIN_EXE_squirrel").into(),
        args: args.iter().map(OsString::from).collect(),
        ready_timeout: Duration::from_secs(120),
        request_timeout: Duration::from_secs(60),
        max_requests: 1000,
        idle: Duration::from_secs(600),
        restart_backoff: squirrel_worker::ml_client::RESTART_BACKOFF,
        max_restart_backoff: squirrel_worker::ml_client::MAX_RESTART_BACKOFF,
    }
}

fn no_models() -> MlChildConfig {
    config(&[
        "--groups".as_ref(),
        "".as_ref(),
        "--models-dir".as_ref(),
        "/nonexistent".as_ref(),
    ])
}

#[tokio::test]
async fn a_child_without_models_answers_ping() {
    let client = MlClient::new(MlBackend::Subprocess(no_models()));
    assert_eq!(
        client.request(MlOp::Ping).await.unwrap(),
        MlBody::Ok(MlOutput::Pong {})
    );
    let body = client
        .request(MlOp::ClipImage {
            input: "/x.webp".into(),
        })
        .await
        .unwrap();
    assert!(matches!(body, MlBody::Failed { .. }), "{body:?}");
    client.stop().await;
}

#[tokio::test]
async fn a_child_with_a_broken_models_dir_never_gets_ready() {
    let dir = tempfile::tempdir().unwrap();
    let client = MlClient::new(MlBackend::Subprocess(config(&[
        "--groups".as_ref(),
        "faces".as_ref(),
        "--models-dir".as_ref(),
        dir.path().as_os_str(),
    ])));
    let err = client.request(MlOp::Ping).await.unwrap_err();
    assert!(err.to_string().contains("ready"), "{err}");
}

/// The address-space limit is applied before anything is loaded (Linux only:
/// macOS has no RLIMIT_AS).
#[cfg(target_os = "linux")]
#[tokio::test]
async fn the_child_runs_under_its_address_space_limit() {
    let client = MlClient::new(MlBackend::Subprocess(no_models()));
    client.request(MlOp::Ping).await.unwrap();
    let pid = client.child_id().await.unwrap();
    let limits = std::fs::read_to_string(format!("/proc/{pid}/limits")).unwrap();
    let line = limits
        .lines()
        .find(|l| l.starts_with("Max address space"))
        .unwrap();
    assert!(
        line.contains(&(4096u64 * 1024 * 1024).to_string()),
        "{line}"
    );
    let core = limits
        .lines()
        .find(|l| l.starts_with("Max core file size"))
        .unwrap();
    assert!(core.split_whitespace().nth(4) == Some("0"), "{core}");
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    assert!(
        status
            .lines()
            .any(|l| l.split_whitespace().eq(["NoNewPrivs:", "1"])),
        "{status}"
    );
    client.stop().await;
}

/// With ONNX Runtime and the models: the CLIP image tower answers with a
/// 512-d unit vector.
#[tokio::test]
async fn clip_image_returns_512_floats() {
    let Some((dylib, models)) = squirrel_ml::testutil::test_ml() else {
        return;
    };
    let client = MlClient::new(MlBackend::Subprocess(config(&[
        "--groups".as_ref(),
        "clip-image,faces".as_ref(),
        "--models-dir".as_ref(),
        models.path().as_os_str(),
        "--ort-dylib".as_ref(),
        dylib.as_os_str(),
        "--threads".as_ref(),
        "2".as_ref(),
    ])));
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("preview.jpg");
    image_fixture(&input);
    let body = client.request(MlOp::ClipImage { input }).await.unwrap();
    let MlBody::Ok(MlOutput::Embedding { embedding }) = body else {
        panic!("{body:?}");
    };
    assert_eq!(embedding.len(), 512);
    let norm: f32 = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-4, "{norm}");
    client.stop().await;
}

fn image_fixture(path: &std::path::Path) {
    let bytes = squirrel_media::testutil::jpeg_sized_with_exif(
        320,
        240,
        &squirrel_media::testutil::ExifSpec::default(),
    );
    std::fs::write(path, bytes).unwrap();
}
