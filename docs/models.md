# 🧠 AI Models and Usage (NeuroChain)

NeuroChain uses ONNX models for classification on CPU (`tract-onnx`). These models do not generate text; they return a label that you can use in script logic or macro templating.

Note: this repo may not include the binary model files in Git. If `models/` is missing `model.onnx` files, download the model pack once:

- `bash scripts/fetch_models.sh` (Linux / macOS / WSL)
- `powershell -ExecutionPolicy Bypass -File scripts/fetch_models.ps1` (Windows PowerShell)

## Licensing note

- Code is Apache-2.0 (see `LICENSE` and `NOTICE` at the repo root).
- Model files are distributed separately; see `models/LICENSE` and `models/THIRD_PARTY_NOTICES.md` for per-model provenance and third-party notices.

## 1) Load a model: `AI:`

```nc
AI: "models/distilbert-sst2/model.onnx"
```

When `AI:` is set, `set x from AI:` uses the active model.

## 2) AI inference into a variable: `set X from AI:`

```nc
AI: "models/distilbert-sst2/model.onnx"
set mood from AI: "I love this movie."
neuro mood
```

## 3) Supported model types and labels

### SST‑2 (Sentiment)
- Path: `models/distilbert-sst2/model.onnx`
- Labels: `Positive`, `Negative`

```nc
AI: "models/distilbert-sst2/model.onnx"
set mood from AI: "This is amazing!"
if mood == "Positive":
    neuro "Great"
else:
    neuro "Bad"
```

### Toxic (Toxicity)
- Path: `models/toxic_quantized/model.onnx`
- Labels: `Toxic`, `Not toxic`

```nc
AI: "models/toxic_quantized/model.onnx"
set tox from AI: "You are stupid"
if tox == "Toxic":
    neuro "Warning"
```

### FactCheck
- Path: `models/factcheck/model.onnx`
- Labels: `entailment`, `neutral`, `contradiction`

```nc
AI: "models/factcheck/model.onnx"
set fact from AI: "Birds fly. | Penguins fly."
if fact == "contradiction":
    neuro "Contradiction detected"
```

### Intent (Command intent)
- Path: `models/intent/model.onnx`
- Labels: `RightCommand`, `LeftCommand`, `UpCommand`, `DownCommand`, `GoCommand`, `StopCommand`, `OtherCommand`
- Base model: `distilbert/distilbert-base-uncased` (Apache-2.0)
- Training data: in-house dataset created for NeuroChain (not distributed)

```nc
AI: "models/intent/model.onnx"
set cmd from AI: "Stop now"
if cmd == "StopCommand":
    neuro "Stopping process"
```

### MacroIntent (macro intent classification)
- Path: `models/intent_macro/model.onnx`
- Labels: `Loop`, `Branch`, `Arith`, `Concat`, `RoleFlag`, `AIBridge`, `DocPrint`, `SetVar`, `Unknown`
- Base model: `distilbert/distilbert-base-uncased` (Apache-2.0)
- Training data: in-house dataset created for NeuroChain (not distributed)

This is used by `macro from AI:` to select a deterministic DSL template.

```nc
AI: "models/intent_macro/model.onnx"
macro from AI: Show Ping 2 times
macro from AI: "If score >= 10 say Congrats else say Nope"
```

## 4) Multi-model usage in one script

The same script can switch models using `AI:` (e.g. toxic → sst2 → factcheck → intent).  
Macro intent stays usable as long as the macro model is loaded (see `examples/macro_test_multimodel.nc`).

## 5) Server (REST)

Server endpoint:
- `POST /api/analyze`

The `model` field can be e.g. `sst2`, `toxic`, `factcheck`, `intent`, `macro` (aliases: `intent_macro`, `macro_intent`).

If the request `content` does not include an `AI:` line, the server injects the model path automatically.

## 6) Paths and settings

- `NC_MODELS_DIR`: models root directory (server default `/opt/neurochain/models`, locally often `models`)
- `NC_MACRO_MODEL` / `NC_MACRO_MODEL_PATH`: overrides the macro intent model path
- `NC_INTENT_THRESHOLD`: macro intent threshold (default `0.35`)

Note: the model directory must also contain `tokenizer.json` (NeuroChain uses it for tokenization).

## Maintainers: publish the model pack (GitHub Releases)

This repo keeps model binaries out of Git. The recommended distribution is a GitHub Release asset zip.

Checklist:

1) Create `neurochain-models-<version>.zip` with a top-level `models/` directory.
2) Include model folders (ONNX + tokenizers, and any config/labels you ship).
3) Include `models/LICENSE` and `models/THIRD_PARTY_NOTICES.md` in the zip (so offline users keep the notices).
4) Exclude `models/manifest.json` from the zip (to avoid overwriting local manifests on extract).
5) Compute SHA256 for the zip and update `models/manifest.json`:
   - `models_zip_url` (GitHub Release asset URL)
   - `models_zip_sha256`
6) Smoke test:
   - `bash scripts/fetch_models.sh` (Linux / macOS / WSL)
   - `powershell -ExecutionPolicy Bypass -File scripts/fetch_models.ps1` (Windows PowerShell)

## 7) See also

- `docs/macros.md` – MacroIntent (macro → DSL → run)
- `docs/language.md` – DSL syntax and best practices
- `docs/troubleshooting.md` – common issues (WSL/target/env)
