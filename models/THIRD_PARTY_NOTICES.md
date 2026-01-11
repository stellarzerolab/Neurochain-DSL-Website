# Third-party model notices

This repository can include third-party model files (ONNX weights, tokenizers, configs).
Those files may have their own licenses and attribution requirements.

## distilbert-sst2 (SST‑2 sentiment)

- Path: `models/distilbert-sst2/`
- Origin: Hugging Face model based on `distilbert-base-uncased` fine-tuned on SST‑2
- Upstream reference: `distilbert/distilbert-base-uncased-finetuned-sst-2-english`
  - https://huggingface.co/distilbert/distilbert-base-uncased-finetuned-sst-2-english
- License: check the upstream model card and license (often Apache-2.0 for DistilBERT-based models)

If you redistribute this model, keep the upstream attribution and comply with the upstream license terms.

## toxic_quantized (Toxicity)

NeuroChain’s toxicity model can be a first-party fine-tune, but it can still carry third-party attribution
requirements via the training data and upstream base model.

- Path: `models/toxic_quantized/`
- Base model: `distilbert-base-uncased` (Apache-2.0; see upstream model card)
  - https://huggingface.co/distilbert/distilbert-base-uncased
- Training data:
  - Jigsaw Toxic Comment Classification Challenge (Kaggle / Hugging Face mirrors)
  - Dataset listings often describe the comments as originating from Wikipedia talk pages.
    Wikipedia content is licensed under CC BY-SA 3.0, which requires attribution. Review the dataset
    card/terms for the exact requirements applicable to your distribution.

Suggested references (verify the exact dataset you used):
- https://www.kaggle.com/c/jigsaw-toxic-comment-classification-challenge
- https://huggingface.co/datasets/thesofakillers/jigsaw-toxic-comment-classification-challenge

If you redistribute this model pack, keep these notices and review the dataset terms you trained on.

## factcheck (NLI / FactCheck)

NeuroChain’s fact-check model can be a first-party fine-tune, but it can still carry third-party attribution
requirements via the training data and upstream base model.

- Path: `models/factcheck/`
- Base model: `distilbert-base-uncased` (Apache-2.0; see upstream model card)
  - https://huggingface.co/distilbert/distilbert-base-uncased
- Training data:
  - MultiNLI / Multi-Genre Natural Language Inference (MultiNLI)
  - MultiNLI is described as a mixture of sources and licenses. Review the dataset card for the
    exact breakdown (e.g. portions under CC BY / CC BY-SA, public domain, etc).

Suggested reference (verify the exact dataset you used):
- https://huggingface.co/datasets/nyu-mll/multi_nli

If you redistribute this model pack, keep these notices and review the dataset terms you trained on.

## intent / intent_macro (first-party fine-tunes)

These models are first-party fine-tunes, but they are based on a third-party base model.

- Paths: `models/intent/`, `models/intent_macro/`
- Base model: `distilbert/distilbert-base-uncased` (Apache-2.0)
  - https://huggingface.co/distilbert/distilbert-base-uncased
- Training data: in-house dataset created for NeuroChain (not distributed as part of the model pack)

---

Note: this file is for transparency and practical compliance. It is not legal advice.
