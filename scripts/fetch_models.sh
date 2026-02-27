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
sha256sums_path="${tmp_dir}/SHA256SUMS"
sha256sig_path="${tmp_dir}/SHA256SUMS.sig"
sha256pem_path="${tmp_dir}/SHA256SUMS.pem"

cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

download_file() {
  local src="$1"
  local dst="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -L --fail --retry 3 --retry-delay 1 -o "${dst}" "${src}"
  elif command -v wget >/dev/null 2>&1; then
    wget -O "${dst}" "${src}"
  else
    echo "ERROR: neither curl nor wget is available."
    exit 1
  fi
}

sha256_file() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${file}" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${file}" | awk '{print $1}'
  else
    return 1
  fi
}

echo "Downloading model pack..."
echo "  url: ${url}"
download_file "${url}" "${zip_path}"

if [[ -z "${sha256}" || "${sha256}" == "TODO" ]]; then
  echo "ERROR: models_zip_sha256 is not set in ${manifest}."
  exit 1
fi

echo "Verifying SHA256 (manifest)..."
if ! got="$(sha256_file "${zip_path}")"; then
  echo "ERROR: no SHA256 tool found. Install sha256sum or shasum."
  exit 1
fi
expected="$(echo "${sha256}" | tr '[:upper:]' '[:lower:]')"
got="$(echo "${got}" | tr '[:upper:]' '[:lower:]')"
if [[ "${got}" != "${expected}" ]]; then
  echo "ERROR: SHA256 mismatch"
  echo "  expected: ${expected}"
  echo "  got:      ${got}"
  exit 1
fi
echo "Manifest SHA256 check: OK"

url_no_query="${url%%\?*}"
if [[ ! "${url_no_query}" =~ ^https://github\.com/([^/]+)/([^/]+)/releases/download/([^/]+)/([^/]+)$ ]]; then
  echo "ERROR: models_zip_url must be a GitHub release asset URL to verify signed SHA256SUMS."
  exit 1
fi
gh_owner="${BASH_REMATCH[1]}"
gh_repo="${BASH_REMATCH[2]}"
gh_tag="${BASH_REMATCH[3]}"
zip_asset_name="${BASH_REMATCH[4]}"
release_base="https://github.com/${gh_owner}/${gh_repo}/releases/download/${gh_tag}"

echo "Checking signed release checksums..."
download_file "${release_base}/SHA256SUMS" "${sha256sums_path}"
download_file "${release_base}/SHA256SUMS.sig" "${sha256sig_path}"
download_file "${release_base}/SHA256SUMS.pem" "${sha256pem_path}"

if ! command -v cosign >/dev/null 2>&1; then
  echo "ERROR: cosign is required for signed checksum verification."
  echo "Install cosign from https://github.com/sigstore/cosign/releases/latest and run again."
  exit 1
fi

identity_regex="^https://github.com/${gh_owner}/${gh_repo}/.github/workflows/release_sha256sums.yml@refs/(heads/main|tags/.*)$"
echo "Verifying SHA256SUMS signature (cosign)..."
cosign verify-blob \
  --certificate "${sha256pem_path}" \
  --signature "${sha256sig_path}" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp "${identity_regex}" \
  "${sha256sums_path}" >/dev/null
echo "SHA256SUMS signature check: OK"

expected_signed="$(
  awk -v target="${zip_asset_name}" '
    {
      f=$2
      sub(/^\*/, "", f)
      if (f == target) { print $1; exit }
    }
  ' "${sha256sums_path}" | tr '[:upper:]' '[:lower:]'
)"
if [[ -z "${expected_signed}" ]]; then
  echo "ERROR: ${zip_asset_name} not found in signed SHA256SUMS."
  exit 1
fi

if ! got_signed="$(sha256_file "${zip_path}")"; then
  echo "ERROR: no SHA256 tool found. Install sha256sum or shasum."
  exit 1
fi
got_signed="$(echo "${got_signed}" | tr '[:upper:]' '[:lower:]')"
if [[ "${got_signed}" != "${expected_signed}" ]]; then
  echo "ERROR: signed SHA256SUMS mismatch"
  echo "  expected: ${expected_signed}"
  echo "  got:      ${got_signed}"
  exit 1
fi
echo "Signed SHA256SUMS check: OK"

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
