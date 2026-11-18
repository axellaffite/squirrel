//! The supervisor against shell scripts standing in for `squirrel ml-exec`.

use std::os::unix::fs::PermissionsExt;

use squirrel_ml::protocol::MlOutput;

use super::*;

/// Says ready, then answers `ping` with ok; `clip_image` runs `on_image`
/// (a shell snippet) instead.
fn fake_child(dir: &std::path::Path, on_image: &str) -> MlChildConfig {
    let path = dir.join("fake-squirrel");
    let script = format!(
        r#"#!/bin/sh
[ "$1" = ml-exec ] || exit 64
echo '{{"ready":true,"groups":[],"onnxruntime":null}}'
while IFS= read -r line; do
  id=$(echo "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
  case "$line" in
    *clip_image*) {on_image} ;;
    *) echo "{{\"id\":$id,\"status\":\"ok\"}}" ;;
  esac
done
"#
    );
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    MlChildConfig {
        program: path,
        args: Vec::new(),
        ready_timeout: Duration::from_secs(10),
        request_timeout: Duration::from_secs(10),
        max_requests: 1000,
        idle: Duration::from_secs(600),
        restart_backoff: RESTART_BACKOFF,
        max_restart_backoff: MAX_RESTART_BACKOFF,
    }
}

fn client(config: MlChildConfig) -> MlClient {
    MlClient::new(MlBackend::Subprocess(config))
}

fn image() -> MlOp {
    MlOp::ClipImage {
        input: "/x.webp".into(),
    }
}

fn is_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

async fn wait_dead(pid: u32) {
    for _ in 0..100 {
        if !is_alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("process {pid} is still alive");
}

#[tokio::test]
async fn one_child_answers_many_requests() {
    let dir = tempfile::tempdir().unwrap();
    let client = client(fake_child(dir.path(), "exit 1"));
    assert_eq!(client.child_id().await, None, "started lazily");
    let body = client.request(MlOp::Ping).await.unwrap();
    assert_eq!(body, MlBody::Ok(MlOutput::Pong {}));
    let pid = client.child_id().await.unwrap();
    client.request(MlOp::Ping).await.unwrap();
    assert_eq!(client.child_id().await, Some(pid), "long-lived");
    client.stop().await;
    wait_dead(pid).await;
}

#[tokio::test]
async fn a_child_killed_mid_request_fails_it_and_the_next_one_gets_a_fresh_child() {
    let dir = tempfile::tempdir().unwrap();
    let client = client(fake_child(dir.path(), "kill -9 $$"));
    client.request(MlOp::Ping).await.unwrap();
    let first = client.child_id().await.unwrap();
    let err = client.request(image()).await.unwrap_err();
    assert!(matches!(err, MlClientError::Crashed(_)), "{err}");
    assert_eq!(client.child_id().await, None);
    client.request(MlOp::Ping).await.unwrap();
    let second = client.child_id().await.unwrap();
    assert_ne!(first, second);
}

#[tokio::test]
async fn a_request_that_times_out_kills_the_child() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = fake_child(dir.path(), "sleep 30");
    config.request_timeout = Duration::from_millis(300);
    let client = client(config);
    client.request(MlOp::Ping).await.unwrap();
    let pid = client.child_id().await.unwrap();
    let err = client.request(image()).await.unwrap_err();
    assert!(matches!(err, MlClientError::TimedOut(_)), "{err}");
    wait_dead(pid).await;
    client.request(MlOp::Ping).await.unwrap();
}

#[tokio::test]
async fn a_garbage_response_is_a_protocol_error() {
    let dir = tempfile::tempdir().unwrap();
    let client = client(fake_child(dir.path(), "echo garbage"));
    let err = client.request(image()).await.unwrap_err();
    assert!(matches!(err, MlClientError::Protocol(_)), "{err}");
    assert_eq!(
        client.child_id().await,
        None,
        "restarted on the next request"
    );
    client.request(MlOp::Ping).await.unwrap();
}

#[tokio::test]
async fn a_response_to_another_request_is_a_protocol_error() {
    let dir = tempfile::tempdir().unwrap();
    let client = client(fake_child(dir.path(), r#"echo '{"id":999,"status":"ok"}'"#));
    let err = client.request(image()).await.unwrap_err();
    assert!(
        matches!(&err, MlClientError::Protocol(m) if m.contains("999")),
        "{err}"
    );
}

#[tokio::test]
async fn the_idle_timer_stops_the_child() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = fake_child(dir.path(), "exit 1");
    config.idle = Duration::from_millis(200);
    let client = client(config);
    client.request(MlOp::Ping).await.unwrap();
    let pid = client.child_id().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    client.request(MlOp::Ping).await.unwrap();
    assert_eq!(
        client.child_id().await,
        Some(pid),
        "a request resets the timer"
    );
    wait_dead(pid).await;
    assert_eq!(client.child_id().await, None);
}

#[tokio::test]
async fn the_child_is_recycled_after_max_requests() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = fake_child(dir.path(), "exit 1");
    config.max_requests = 2;
    let client = client(config);
    client.request(MlOp::Ping).await.unwrap();
    let first = client.child_id().await.unwrap();
    client.request(MlOp::Ping).await.unwrap();
    assert_eq!(client.child_id().await, None, "recycled after 2");
    wait_dead(first).await;
    client.request(MlOp::Ping).await.unwrap();
    assert_ne!(client.child_id().await, Some(first));
}

#[tokio::test]
async fn a_child_that_never_gets_ready_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("never-ready");
    std::fs::write(&path, "#!/bin/sh\necho 'cannot load models' >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut config = fake_child(dir.path(), "exit 1");
    config.program = path;
    let err = client(config).request(MlOp::Ping).await.unwrap_err();
    assert!(matches!(err, MlClientError::NotReady(_)), "{err}");
}

#[tokio::test]
async fn the_child_sees_none_of_the_workers_environment() {
    assert!(std::env::var_os("DATABASE_URL").is_some());
    let dir = tempfile::tempdir().unwrap();
    let dump = dir.path().join("env.txt");
    let config = fake_child(
        dir.path(),
        &format!(
            r#"env > '{}'; echo "{{\"id\":$id,\"status\":\"ok\"}}""#,
            dump.display()
        ),
    );
    client(config).request(image()).await.unwrap();
    let env = std::fs::read_to_string(&dump).unwrap();
    for secret in ["DATABASE_URL", "SQUIRREL_", "CARGO"] {
        assert!(
            !env.lines().any(|l| l.starts_with(secret)),
            "{secret} leaked: {env}"
        );
    }
}

#[tokio::test]
async fn in_process_answers_ping() {
    let engine = Engine::load(&squirrel_ml::serve::EngineConfig {
        groups: Vec::new(),
        models: squirrel_ml::ModelDir::new("/nonexistent"),
        ort_dylib: None,
        threads: 1,
    })
    .unwrap();
    let client = MlClient::new(MlBackend::InProcess(Arc::new(std::sync::Mutex::new(
        engine,
    ))));
    assert_eq!(
        client.request(MlOp::Ping).await.unwrap(),
        MlBody::Ok(MlOutput::Pong {})
    );
}

#[tokio::test]
async fn failed_starts_back_off_with_a_growing_delay() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken");
    std::fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut config = fake_child(dir.path(), "exit 1");
    config.program = path.clone();
    config.restart_backoff = Duration::from_millis(200);
    config.max_restart_backoff = Duration::from_millis(300);
    let client = client(config);
    let err = client.request(MlOp::Ping).await.unwrap_err();
    assert!(matches!(err, MlClientError::NotReady(_)), "{err}");
    let err = client.request(MlOp::Ping).await.unwrap_err();
    let wait = err.retry_after().expect("a backoff");
    assert!(wait <= Duration::from_millis(200), "{wait:?}");
    tokio::time::sleep(wait).await;
    assert!(matches!(
        client.request(MlOp::Ping).await.unwrap_err(),
        MlClientError::NotReady(_)
    ));
    // Doubled, capped at 300 ms.
    let wait = client
        .request(MlOp::Ping)
        .await
        .unwrap_err()
        .retry_after()
        .unwrap();
    assert!(
        wait > Duration::from_millis(200) && wait <= Duration::from_millis(300),
        "{wait:?}"
    );
    // Fixed install: the next start after the pause works and resets the delay.
    std::fs::copy(dir.path().join("fake-squirrel"), &path).unwrap();
    tokio::time::sleep(wait).await;
    client.request(MlOp::Ping).await.unwrap();
}

/// A request whose future is dropped mid-exchange (job timeout, shutdown)
/// must not leave its late answer to be read as the next request's.
#[tokio::test]
async fn a_cancelled_request_does_not_poison_the_next_one() {
    let dir = tempfile::tempdir().unwrap();
    // Answers clip_image after a pause, as a slow inference would.
    let client = client(fake_child(
        dir.path(),
        r#"sleep 0.5; echo "{\"id\":$id,\"status\":\"ok\",\"embedding\":[1.0]}""#,
    ));
    client.request(MlOp::Ping).await.unwrap();
    let first = client.child_id().await.unwrap();
    let cancelled = tokio::time::timeout(Duration::from_millis(100), client.request(image())).await;
    assert!(cancelled.is_err(), "dropped before the answer");
    tokio::time::sleep(Duration::from_millis(600)).await;
    // The late answer to the cancelled request is never read as this one's.
    assert_eq!(
        client.request(MlOp::Ping).await.unwrap(),
        MlBody::Ok(MlOutput::Pong {})
    );
    assert_ne!(
        client.child_id().await,
        Some(first),
        "the child was replaced"
    );
    wait_dead(first).await;
}

/// Stopping kills the child's whole process group, not just the child.
#[tokio::test]
async fn stopping_kills_what_the_child_started() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("grandchild");
    let mut config = fake_child(dir.path(), "exit 1");
    let script = std::fs::read_to_string(&config.program).unwrap().replacen(
        "echo '{\"ready\"",
        &format!(
            "sleep 30 &\necho $! > '{}'\necho '{{\"ready\"",
            pid_file.display()
        ),
        1,
    );
    let path = dir.path().join("with-grandchild");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    config.program = path;
    let client = client(config);
    client.request(MlOp::Ping).await.unwrap();
    let grandchild: u32 = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(is_alive(grandchild));
    client.stop().await;
    wait_dead(grandchild).await;
}

#[test]
fn child_logs_are_rate_limited_per_window() {
    let start = Instant::now();
    let mut budget = LogBudget::new(start);
    let admitted = (0..30).filter(|_| budget.admit(start).0).count();
    assert_eq!(admitted, 20);
    let (admitted, dropped) = budget.admit(start + Duration::from_secs(61));
    assert!(admitted);
    assert_eq!(dropped, 10, "the previous window's drops are reported once");
    assert_eq!(budget.admit(start + Duration::from_secs(62)).1, 0);
}

/// Waiting for the child is bounded: a request queued behind others gives
/// up with a retryable Busy instead of waiting forever.
#[tokio::test]
async fn waiting_for_a_busy_child_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = fake_child(
        dir.path(),
        r#"sleep 0.8; echo "{\"id\":$id,\"status\":\"ok\"}""#,
    );
    config.ready_timeout = Duration::from_millis(100);
    config.request_timeout = Duration::from_millis(1000);
    let client = Arc::new(client(config));
    client.request(MlOp::Ping).await.unwrap();
    let requests: Vec<_> = (0..3)
        .map(|_| {
            let client = client.clone();
            tokio::spawn(async move { client.request(image()).await })
        })
        .collect();
    let mut busy = 0;
    for request in requests {
        match request.await.unwrap() {
            Ok(_) => {}
            Err(err @ MlClientError::Busy(_)) => {
                assert!(err.retry_after().is_some());
                busy += 1;
            }
            Err(other) => panic!("{other}"),
        }
    }
    assert!(busy >= 1, "the third request should give up");
}
