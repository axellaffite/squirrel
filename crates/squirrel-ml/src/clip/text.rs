//! The SigLIP2 text tower: text in any language → cleaned like open_clip's
//! `canonicalize` → Gemma tokens (64, `<eos>`-terminated, 0-padded) → 768-d
//! unit vector.

use ndarray::Array2;
use ort::session::Session;
use ort::value::TensorElementType;
use ort::value::TensorRef;
use tokenizers::Tokenizer;

use super::{ClipError, EMBEDDING_DIM, normalize};
use crate::models::ModelDir;
use crate::runtime::{self, RuntimeError};

pub const TEXT_MODEL: &str = "siglip2/textual/model.onnx";
pub const TOKENIZER: &str = "siglip2/textual/tokenizer.json";
/// The model's context (open_clip `text_cfg.context_length`, `config.json`).
pub const CONTEXT: usize = 64;
/// `<eos>`, appended after the text (the tokenizer's post-processor).
const EOS: u32 = 1;
/// `<pad>` (`tokenizer_config.json`), fills the rest of the context.
const PAD: u32 = 0;

pub struct ClipText {
    session: Session,
    tokenizer: Tokenizer,
    /// The model's input element type (int32 or int64).
    input_type: TensorElementType,
}

impl ClipText {
    pub fn load(models: &ModelDir, threads: usize) -> Result<Self, RuntimeError> {
        let path = models.file(TOKENIZER);
        let tokenizer = Tokenizer::from_bytes(models.read_verified(TOKENIZER)?).map_err(|err| {
            RuntimeError::Session {
                path: path.clone(),
                message: err.to_string(),
            }
        })?;
        let model = models.file(TEXT_MODEL);
        let session =
            runtime::session_from_memory(&model, &models.read_verified(TEXT_MODEL)?, threads)?;
        let input_type = session
            .inputs()
            .first()
            .and_then(|input| input.dtype().tensor_type())
            .ok_or_else(|| RuntimeError::Session {
                path: model.clone(),
                message: "the text model has no tensor input".into(),
            })?;
        Ok(Self {
            session,
            tokenizer,
            input_type,
        })
    }

    /// Cleaned text's tokens + `<eos>`, cut to 64 (the `<eos>` is kept),
    /// padded with `<pad>`: what open_clip's HF tokenizer feeds the model.
    pub fn tokens(&self, text: &str) -> Result<Vec<u32>, ClipError> {
        let encoding = self
            .tokenizer
            .encode(canonicalize(text), false)
            .map_err(|err| ClipError::Inference(err.to_string()))?;
        Ok(frame(encoding.get_ids()))
    }

    pub fn embed(&mut self, text: &str) -> Result<Vec<f32>, ClipError> {
        let tokens = self.tokens(text)?;
        let fail = |err: ort::Error| ClipError::Inference(err.to_string());
        let outputs = match self.input_type {
            TensorElementType::Int64 => {
                let ids = Array2::from_shape_fn((1, CONTEXT), |(_, i)| i64::from(tokens[i]));
                let tensor = TensorRef::from_array_view(ids.view()).map_err(fail)?;
                self.session.run(ort::inputs![tensor]).map_err(fail)?
            }
            TensorElementType::Int32 => {
                let ids = Array2::from_shape_fn((1, CONTEXT), |(_, i)| tokens[i] as i32);
                let tensor = TensorRef::from_array_view(ids.view()).map_err(fail)?;
                self.session.run(ort::inputs![tensor]).map_err(fail)?
            }
            other => {
                return Err(ClipError::Inference(format!(
                    "unexpected text input type {other:?}"
                )));
            }
        };
        let (_, values) = outputs[0].try_extract_tensor::<f32>().map_err(fail)?;
        if values.len() != EMBEDDING_DIM {
            return Err(ClipError::Inference(format!(
                "expected {EMBEDDING_DIM} values, got {}",
                values.len()
            )));
        }
        Ok(normalize(values))
    }
}

fn frame(ids: &[u32]) -> Vec<u32> {
    let body = &ids[..ids.len().min(CONTEXT - 1)];
    let mut tokens = Vec::with_capacity(CONTEXT);
    tokens.extend_from_slice(body);
    tokens.push(EOS);
    tokens.resize(CONTEXT, PAD);
    tokens
}

/// open_clip's `canonicalize_text` (the cleaning SigLIP was trained with):
/// `_` becomes a space, ASCII punctuation goes, lower case, single spaces.
/// (Its ftfy/HTML pre-pass is left out: queries and labels are plain text.)
pub fn canonicalize(text: &str) -> String {
    let kept: String = text
        .chars()
        .map(|c| if c == '_' { ' ' } else { c })
        .filter(|c| !c.is_ascii_punctuation())
        .collect();
    kept.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_end_with_eos_and_are_padded_to_64() {
        let tokens = frame(&[10, 11, 12]);
        assert_eq!(tokens.len(), CONTEXT);
        assert_eq!(&tokens[..4], [10, 11, 12, EOS]);
        assert!(tokens[4..].iter().all(|t| *t == PAD));
        let long: Vec<u32> = (100..300).collect();
        let tokens = frame(&long);
        assert_eq!(tokens.len(), CONTEXT);
        assert_eq!(tokens[CONTEXT - 1], EOS, "the eos survives truncation");
        assert_eq!(tokens[CONTEXT - 2], 162);
    }

    #[test]
    fn text_is_cleaned_like_open_clip() {
        assert_eq!(canonicalize("  A photo of a DOG.  "), "a photo of a dog");
        assert_eq!(canonicalize("red_circle, (big)!"), "red circle big");
        assert_eq!(
            canonicalize("Cercle   ROUGE à l'école"),
            "cercle rouge à lécole"
        );
        assert_eq!(canonicalize("ÉTÉ"), "été");
        assert_eq!(canonicalize("?!"), "");
    }

    /// Ids recorded once from the pinned Gemma tokenizer (`tokenizers` reads
    /// it as is: no new crate).
    #[test]
    fn the_real_tokenizer_matches_gemma() {
        let Some(models) = crate::testutil::test_models_for(&[crate::models::ModelGroup::ClipText])
        else {
            return;
        };
        let tokenizer = Tokenizer::from_file(models.file(TOKENIZER)).unwrap();
        let ids = tokenizer
            .encode(canonicalize("A photo of a DOG.").as_str(), false)
            .unwrap();
        // "a", "▁photo", "▁of", "▁a", "▁dog" (Gemma: "▁of" 576, "▁a" 476).
        assert_eq!(ids.get_ids(), [235250, 2686, 576, 476, 5929]);
        let with_specials = tokenizer.encode("a dog", true).unwrap();
        assert_eq!(
            with_specials.get_ids().last(),
            Some(&EOS),
            "the post-processor appends <eos>, as frame() does"
        );
        let ids = frame(
            tokenizer
                .encode(canonicalize(&"chien ".repeat(300)).as_str(), false)
                .unwrap()
                .get_ids(),
        );
        assert_eq!((ids.len(), ids[CONTEXT - 1]), (CONTEXT, EOS));
    }
}
