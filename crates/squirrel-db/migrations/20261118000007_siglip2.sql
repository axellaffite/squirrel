-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 5, Milestone I: the image–text model becomes SigLIP2 ViT-B/16
-- ('siglip2-vit-b16-224', 768 dimensions) instead of OpenAI CLIP ViT-B/32
-- ('openai-vit-b32-224', 512). Vectors of two models cannot be compared, so
-- everything derived from the old one goes and is computed again:
-- * every stored embedding (only the old model was ever stored, and the
--   column type change needs every row to have the new size);
-- * the label bank and its digests (the start-up queues a new `label_bank`);
-- * the `clip` tags (re-tagged after each new embedding);
-- * images embedded or failed go back to `pending`: the worker's start-up
--   backfill queues `clip_embed` for every ready image without an embedding
--   of the current model. `unsupported` stays: it is about the input.
DELETE FROM asset_tags WHERE source = 'clip';
DELETE FROM clip_embeddings;
DELETE FROM clip_label_embeddings;
DELETE FROM clip_label_banks;

-- SigLIP2 ViT-B/16: 768 dimensions. halfvec(768) is 1 544 bytes: stored inline.
ALTER TABLE clip_embeddings ALTER COLUMN embedding TYPE halfvec(768);
ALTER TABLE clip_label_embeddings ALTER COLUMN embedding TYPE halfvec(768);

UPDATE media SET clip_state = 'pending', updated_at = now()
 WHERE kind = 'image' AND clip_state IN ('ready', 'failed');
