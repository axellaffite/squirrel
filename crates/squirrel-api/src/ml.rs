//! The server's CLIP text encoder for photo search: a lazily started
//! `ml-exec` child with the text tower, stopped after 5 idle minutes. Its
//! supervisor bounds every wait (start, request, queue), backs off after a
//! failed start and replaces a child left mid-exchange, so a search never
//! holds up other requests: at worst it answers 503.

use std::sync::Arc;
use std::time::Duration;

use squirrel_ml::client::{MlClient, MlClientError};
use squirrel_ml::clip::check_embedding;
use squirrel_ml::protocol::{MlBody, MlOp, MlOutput};

/// The text child frees its ≈ 0.4 GB after this long without a search.
pub const TEXT_IDLE: Duration = Duration::from_secs(5 * 60);
/// One query's encoding (tens of milliseconds when warm).
pub const TEXT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest search text, in characters.
pub const MAX_QUERY_CHARS: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    #[error("the search text must be 1–{MAX_QUERY_CHARS} characters")]
    Length,
    /// The child cannot answer now (starting, backing off, busy, crashed).
    #[error("text search is unavailable: {0}")]
    Unavailable(String),
}

pub struct TextEncoder {
    /// Shared with the tasks requests run in.
    client: Arc<MlClient>,
}

impl TextEncoder {
    pub fn new(client: MlClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// The running child's process id (tests).
    #[doc(hidden)]
    pub async fn child_id(&self) -> Option<u32> {
        self.client.child_id().await
    }

    /// The unit 512-d embedding of `text` (trimmed, 1–200 characters).
    pub async fn encode(&self, text: &str) -> Result<Vec<f32>, EncodeError> {
        let text = text.trim();
        let chars = text.chars().count();
        if chars == 0 || chars > MAX_QUERY_CHARS {
            return Err(EncodeError::Length);
        }
        let op = MlOp::ClipText {
            texts: vec![text.to_owned()],
        };
        let unavailable = |why: String| EncodeError::Unavailable(why);
        // Its own task: an HTTP request cancelled mid-search (client gone,
        // timeout) does not cancel the exchange with the shared child.
        let client = self.client.clone();
        let answer = tokio::spawn(async move { client.request(op).await })
            .await
            .map_err(|err| unavailable(err.to_string()))?
            .map_err(|err: MlClientError| unavailable(err.to_string()))?;
        let embedding = match answer {
            MlBody::Ok(MlOutput::Embeddings { mut embeddings }) if embeddings.len() == 1 => {
                embeddings.remove(0)
            }
            other => return Err(unavailable(format!("unexpected answer {other:?}"))),
        };
        check_embedding(&embedding)
            .map_err(|why| unavailable(format!("invalid embedding: {why}")))?;
        Ok(embedding)
    }

    /// Starts the child in the background (the search page opens): the first
    /// query then skips the model load. Returns at once.
    pub fn warm_up(self: &Arc<Self>) {
        let encoder = self.clone();
        tokio::spawn(async move {
            if let Err(err) = encoder.client.health().await {
                tracing::warn!(error = %err, "warming up the text encoder failed");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use squirrel_ml::client::MlBackend;

    fn fake(
        answer: impl Fn(&MlOp) -> Result<MlBody, MlClientError> + Send + Sync + 'static,
    ) -> TextEncoder {
        TextEncoder::new(MlClient::new(MlBackend::Fake(Arc::new(answer))))
    }

    fn unit() -> Vec<f32> {
        let mut v = vec![0.0; 512];
        v[0] = 1.0;
        v
    }

    #[tokio::test]
    async fn queries_are_bounded_before_any_inference() {
        let encoder = fake(|_| panic!("no inference for a refused query"));
        assert!(matches!(
            encoder.encode("  ").await,
            Err(EncodeError::Length)
        ));
        let long = "x".repeat(MAX_QUERY_CHARS + 1);
        assert!(matches!(
            encoder.encode(&long).await,
            Err(EncodeError::Length)
        ));
    }

    #[tokio::test]
    async fn the_text_is_trimmed_and_the_embedding_checked() {
        let encoder = fake(|op| {
            assert_eq!(
                op,
                &MlOp::ClipText {
                    texts: vec!["a dog".into()]
                }
            );
            Ok(MlBody::Ok(MlOutput::Embeddings {
                embeddings: vec![unit()],
            }))
        });
        assert_eq!(encoder.encode(" a dog ").await.unwrap(), unit());
        let broken = fake(|_| {
            Ok(MlBody::Ok(MlOutput::Embeddings {
                embeddings: vec![vec![0.5; 3]],
            }))
        });
        assert!(matches!(
            broken.encode("dog").await,
            Err(EncodeError::Unavailable(_))
        ));
        let down = fake(|_| Err(MlClientError::Backoff(Duration::from_secs(30))));
        assert!(matches!(
            down.encode("dog").await,
            Err(EncodeError::Unavailable(_))
        ));
    }

    /// A shell script standing in for `squirrel ml-exec --groups clip-text`:
    /// answers each `clip_text` after `delay` seconds with a unit vector.
    fn script_child(dir: &std::path::Path, delay: &str) -> squirrel_ml::client::MlChildConfig {
        use std::os::unix::fs::PermissionsExt;
        let unit: Vec<String> = unit().iter().map(|v| v.to_string()).collect();
        let path = dir.join("fake-squirrel");
        let script = format!(
            r#"#!/bin/sh
echo '{{"ready":true,"groups":["clip-text"],"onnxruntime":null}}'
while IFS= read -r line; do
  id=$(echo "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
  case "$line" in
    *clip_text*) sleep {delay}; echo "{{\"id\":$id,\"status\":\"ok\",\"embeddings\":[[{}]]}}" ;;
    *) echo "{{\"id\":$id,\"status\":\"ok\"}}" ;;
  esac
done
"#,
            unit.join(",")
        );
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        squirrel_ml::client::MlChildConfig {
            program: path,
            args: Vec::new(),
            ready_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(10),
            max_requests: 1000,
            idle: Duration::from_secs(600),
            restart_backoff: squirrel_ml::client::RESTART_BACKOFF,
            max_restart_backoff: squirrel_ml::client::MAX_RESTART_BACKOFF,
        }
    }

    /// A search cancelled mid-request (the HTTP client left) neither kills
    /// nor poisons the shared child: the next search reuses it.
    #[tokio::test]
    async fn a_cancelled_search_leaves_the_child_usable() {
        let dir = tempfile::tempdir().unwrap();
        let encoder = TextEncoder::new(MlClient::new(MlBackend::Subprocess(script_child(
            dir.path(),
            "0.3",
        ))));
        encoder.encode("warm").await.unwrap();
        let pid = encoder.child_id().await.unwrap();
        let cancelled =
            tokio::time::timeout(Duration::from_millis(50), encoder.encode("dog")).await;
        assert!(cancelled.is_err(), "cancelled before the answer");
        assert_eq!(encoder.encode("cat").await.unwrap(), unit());
        assert_eq!(encoder.child_id().await, Some(pid), "the same child");
    }

    /// With the real models: a dog is closer to a puppy than to a car.
    #[tokio::test]
    async fn related_texts_are_closer() {
        use squirrel_ml::ModelGroup;
        let Some((dylib, models)) = squirrel_ml::testutil::test_ml_for(&[ModelGroup::ClipText])
        else {
            return;
        };
        let engine = squirrel_ml::serve::Engine::load(&squirrel_ml::serve::EngineConfig {
            groups: vec![ModelGroup::ClipText],
            models,
            ort_dylib: Some(dylib),
            threads: 2,
        })
        .unwrap();
        let encoder = TextEncoder::new(MlClient::new(MlBackend::InProcess(Arc::new(
            std::sync::Mutex::new(engine),
        ))));
        let dog = encoder.encode("a photo of a dog").await.unwrap();
        let puppy = encoder.encode("a puppy").await.unwrap();
        let car = encoder.encode("a car").await.unwrap();
        let cos = squirrel_ml::clip::cosine;
        assert!(cos(&dog, &puppy) > cos(&dog, &car));
    }
}
