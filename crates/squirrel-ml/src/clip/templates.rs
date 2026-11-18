//! Prompt templates for zero-shot tags: a fixed subset of OpenAI's ImageNet
//! prompt ensemble. `{}` is replaced by a class name; a label's embedding is
//! the re-normalised mean over every template and class name.

pub const TEMPLATES: [&str; 7] = [
    "a photo of a {}.",
    "a photo of the {}.",
    "a close-up photo of a {}.",
    "a bright photo of a {}.",
    "a photo of my {}.",
    "a blurry photo of a {}.",
    "a cropped photo of a {}.",
];

/// Every prompt for `names`, template-major.
pub fn prompts(names: &[String]) -> Vec<String> {
    TEMPLATES
        .iter()
        .flat_map(|template| names.iter().map(move |name| template.replace("{}", name)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_has_one_slot() {
        for template in TEMPLATES {
            assert_eq!(template.matches("{}").count(), 1, "{template}");
        }
        let names = vec!["dog".to_owned(), "puppy".to_owned()];
        let prompts = prompts(&names);
        assert_eq!(prompts.len(), 14);
        assert_eq!(prompts[0], "a photo of a dog.");
        assert_eq!(prompts[1], "a photo of a puppy.");
    }
}
