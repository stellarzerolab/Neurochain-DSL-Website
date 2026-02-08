# MacroIntent (macro intent classification)

Expected files after downloading the model pack:

- `models/intent_macro/model.onnx`
- `models/intent_macro/tokenizer.json`
- `models/intent_macro/vocab.txt`

## Verify the download (recommended)

Model files are distributed via the model pack zip. To verify the zip checksum/signature, see `docs/models.md` ("Verify the download") and `models/manifest.json`.

## Licensing and provenance

- Code license: Apache-2.0 (see repo root `LICENSE`).
- Model weights: intended to be Apache-2.0 (see `models/LICENSE`).
- Base model: `distilbert/distilbert-base-uncased` (Apache-2.0)
  - https://huggingface.co/distilbert/distilbert-base-uncased
- Training data: in-house dataset created for NeuroChain (not distributed).
