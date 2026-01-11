# Models (`models/`)

NeuroChain loads ONNX models from `models/` by default (see `docs/models.md`).

This repository intentionally does **not** commit the binary model files (they are large). Instead, the recommended distribution is:

- source code in Git
- model pack as a GitHub Release asset (zip)

## Licensing and attribution

- Code is licensed under Apache-2.0 (see `LICENSE`).
- Models are distributed under Apache-2.0 unless noted otherwise (see `models/LICENSE`).
- Some models can be third-party; see `models/THIRD_PARTY_NOTICES.md`.

## Download models (recommended)

The `models/manifest.json` file contains the URL + SHA256 for the model pack.

- Linux / macOS / WSL:
  - `bash scripts/fetch_models.sh`
- Windows PowerShell:
  - `powershell -ExecutionPolicy Bypass -File scripts/fetch_models.ps1`

Fastest option: download the model pack from the release page and extract the zip, then copy the `models/` folder into the repo root (same level as `Cargo.toml`), replacing the existing `models/` folder.
Release: https://github.com/stellarzerolab/Neurochain-DSL/releases/tag/v0.1.0

After downloading and extracting, you should have folders like:

```
models/distilbert-sst2/model.onnx
models/toxic_quantized/model.onnx
models/factcheck/model.onnx
models/intent/model.onnx
models/intent_macro/model.onnx
```

## Notes

- Some model files may be third-party and can have their own licenses. Check model sources before redistributing.
