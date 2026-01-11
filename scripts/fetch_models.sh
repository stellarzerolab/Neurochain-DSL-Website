#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="${repo_root}/models/manifest.json"

if [[ ! -f "${manifest}" ]]; then
  echo "ERROR: missing ${manifest}"
  exit 1
fi

extract_value() {
  local key="$1"
  # Simple JSON string extraction (works for the tiny manifest schema).
  sed -n "s/.*\"${key}\"[[:space:]]*:[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" "${manifest}" | head -n 1
}

url="$(extract_value "models_zip_url")"
sha256="$(extract_value "models_zip_sha256")"

if [[ -z "${url}" || "${url}" == *"<OWNER>"* || "${url}" == *"<REPO>"* ]]; then
  echo "ERROR: models_zip_url is not set."
  echo "Edit ${manifest} and set models_zip_url to your GitHub Release asset URL."
  exit 1
fi

tmp_dir="$(mktemp -d)"
zip_path="${tmp_dir}/neurochain-models.zip"

cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

echo "Downloading model pack..."
echo "  url: ${url}"

if command -v curl >/dev/null 2>&1; then
  curl -L --fail --retry 3 --retry-delay 1 -o "${zip_path}" "${url}"
elif command -v wget >/dev/null 2>&1; then
  wget -O "${zip_path}" "${url}"
else
  echo "ERROR: neither curl nor wget is available."
  exit 1
fi

if [[ -n "${sha256}" && "${sha256}" != "TODO" ]]; then
  echo "Verifying SHA256..."
  if command -v sha256sum >/dev/null 2>&1; then
    echo "${sha256}  ${zip_path}" | sha256sum -c -
  elif command -v shasum >/dev/null 2>&1; then
    got="$(shasum -a 256 "${zip_path}" | awk '{print $1}')"
    if [[ "${got}" != "${sha256}" ]]; then
      echo "ERROR: SHA256 mismatch"
      echo "  expected: ${sha256}"
      echo "  got:      ${got}"
      exit 1
    fi
  else
    echo "WARN: no SHA256 tool found; skipping checksum verification."
  fi
else
  echo "WARN: models_zip_sha256 is not set (or TODO); skipping checksum verification."
fi

echo "Extracting into repo root: ${repo_root}"

if command -v unzip >/dev/null 2>&1; then
  unzip -o -q "${zip_path}" -d "${repo_root}"
elif command -v python3 >/dev/null 2>&1; then
  REPO_ROOT="${repo_root}" ZIP_PATH="${zip_path}" python3 - <<'PY'
import os
import sys
import zipfile

repo_root = os.environ["REPO_ROOT"]
zip_path = os.environ["ZIP_PATH"]

with zipfile.ZipFile(zip_path) as zf:
    zf.extractall(repo_root)
PY
else
  echo "ERROR: need unzip or python3 to extract zip files."
  exit 1
fi

required=(
  "models/intent_macro/model.onnx"
  "models/distilbert-sst2/model.onnx"
  "models/toxic_quantized/model.onnx"
  "models/factcheck/model.onnx"
  "models/intent/model.onnx"
)

for f in "${required[@]}"; do
  if [[ ! -f "${repo_root}/${f}" ]]; then
    echo "ERROR: expected file not found after extraction: ${f}"
    echo "Hint: the zip should contain a top-level 'models/' directory."
    exit 1
  fi
done

echo "Done. Models are ready under: ${repo_root}/models"
