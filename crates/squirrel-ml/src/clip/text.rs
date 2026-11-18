//! The CLIP text tower: English text → CLIP BPE (77 tokens) → 512-d unit vector.

use ndarray::Array2;
use ort::session::Session;
use ort::value::TensorElementType;
use ort::value::TensorRef;
use tokenizers::Tokenizer;

use super::{ClipError, EMBEDDING_DIM, normalize};
use crate::models::ModelDir;
use crate::runtime::{self, RuntimeError};

pub const TEXT_MODEL: &str = "clip/textual/model.onnx";
pub const TOKENIZER: &str = "clip/textual/tokenizer.json";
/// `model_max_length` of the tokenizer and the model's context.
pub const CONTEXT: usize = 77;
const START: u32 = 49406;
/// End of text, also the padding token (`tokenizer_config.json`).
const END: u32 = 49407;

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

    /// `<|startoftext|>` + tokens + `<|endoftext|>`, cut to 77, padded with
    /// `<|endoftext|>` (the model reads the first end token).
    pub fn tokens(&self, text: &str) -> Result<Vec<u32>, ClipError> {
        let encoding = self
            .tokenizer
            .encode(text, false)
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
    let body = &ids[..ids.len().min(CONTEXT - 2)];
    let mut tokens = Vec::with_capacity(CONTEXT);
    tokens.push(START);
    tokens.extend_from_slice(body);
    tokens.push(END);
    tokens.resize(CONTEXT, END);
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_framed_and_padded_to_77() {
        let tokens = frame(&[1, 2, 3]);
        assert_eq!(tokens.len(), CONTEXT);
        assert_eq!(&tokens[..5], [START, 1, 2, 3, END]);
        assert!(tokens[5..].iter().all(|t| *t == END));
        let long: Vec<u32> = (0..200).collect();
        let tokens = frame(&long);
        assert_eq!(tokens.len(), CONTEXT);
        assert_eq!(tokens[CONTEXT - 1], END);
        assert_eq!(tokens[CONTEXT - 2], 74);
    }

    #[test]
    fn the_real_tokenizer_matches_clip() {
        let Some(models) = crate::testutil::test_models() else {
            return;
        };
        let tokenizer = Tokenizer::from_file(models.file(TOKENIZER)).unwrap();
        let ids = tokenizer.encode("A photo of a DOG.", false).unwrap();
        // CLIP BPE: lower-cased, "</w>"-suffixed words: a photo of a dog .
        assert_eq!(ids.get_ids(), [320, 1125, 539, 320, 1929, 269]);
    }
}
