Param(
  [string]$ManifestPath = ""
)

$ErrorActionPreference = "Stop"

function Fail([string]$Message) {
  Write-Host "ERROR: $Message" -ForegroundColor Red
  exit 1
}

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

if ([string]::IsNullOrWhiteSpace($ManifestPath)) {
  $ManifestPath = Join-Path $RepoRoot "models\manifest.json"
}

if (!(Test-Path $ManifestPath)) {
  Fail "Missing manifest: $ManifestPath"
}

$Manifest = Get-Content -Raw $ManifestPath | ConvertFrom-Json
$Url = [string]$Manifest.models_zip_url
$Sha = [string]$Manifest.models_zip_sha256

if ([string]::IsNullOrWhiteSpace($Url) -or $Url.Contains("<OWNER>") -or $Url.Contains("<REPO>")) {
  Fail "models_zip_url is not set. Edit $ManifestPath and point it to your GitHub Release asset URL."
}

$TempDir = Join-Path $env:TEMP ("neurochain-models-" + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
$ZipPath = Join-Path $TempDir "neurochain-models.zip"
$ShaSumsPath = Join-Path $TempDir "SHA256SUMS"
$ShaSigPath = Join-Path $TempDir "SHA256SUMS.sig"
$ShaPemPath = Join-Path $TempDir "SHA256SUMS.pem"

Write-Host "Downloading model pack..."
Write-Host "  url: $Url"
Invoke-WebRequest -Uri $Url -OutFile $ZipPath -UseBasicParsing

if ([string]::IsNullOrWhiteSpace($Sha) -or $Sha -eq "TODO") {
  Fail "models_zip_sha256 is not set in $ManifestPath."
}
Write-Host "Verifying SHA256 (manifest)..."
$Got = (Get-FileHash -Algorithm SHA256 $ZipPath).Hash.ToLowerInvariant()
$Expected = $Sha.ToLowerInvariant()
if ($Got -ne $Expected) {
  Fail "SHA256 mismatch. Expected $Expected, got $Got."
}
Write-Host "Manifest SHA256 check: OK"

$UrlNoQuery = $Url.Split('?')[0]
$Match = [regex]::Match($UrlNoQuery, '^https://github\.com/([^/]+)/([^/]+)/releases/download/([^/]+)/([^/]+)$')
if (!$Match.Success) {
  Fail "models_zip_url must be a GitHub release asset URL to verify signed SHA256SUMS."
}
$Owner = $Match.Groups[1].Value
$Repo = $Match.Groups[2].Value
$Tag = $Match.Groups[3].Value
$ZipAssetName = $Match.Groups[4].Value
$ReleaseBase = "https://github.com/$Owner/$Repo/releases/download/$Tag"

Write-Host "Checking signed release checksums..."
Invoke-WebRequest -Uri "$ReleaseBase/SHA256SUMS" -OutFile $ShaSumsPath -UseBasicParsing
Invoke-WebRequest -Uri "$ReleaseBase/SHA256SUMS.sig" -OutFile $ShaSigPath -UseBasicParsing
Invoke-WebRequest -Uri "$ReleaseBase/SHA256SUMS.pem" -OutFile $ShaPemPath -UseBasicParsing

$CosignCmd = Get-Command cosign -ErrorAction SilentlyContinue
if ($null -eq $CosignCmd) {
  Fail "cosign is required for signed checksum verification. Install from https://github.com/sigstore/cosign/releases/latest and run again."
}

$IdentityRegex = "^https://github.com/$Owner/$Repo/.github/workflows/release_sha256sums.yml@refs/(heads/main|tags/.*)$"
Write-Host "Verifying SHA256SUMS signature (cosign)..."
& $CosignCmd.Source verify-blob `
  --certificate $ShaPemPath `
  --signature $ShaSigPath `
  --certificate-oidc-issuer https://token.actions.githubusercontent.com `
  --certificate-identity-regexp $IdentityRegex `
  $ShaSumsPath | Out-Null
if ($LASTEXITCODE -ne 0) {
  Fail "cosign verify-blob failed."
}
Write-Host "SHA256SUMS signature check: OK"

$ExpectedSigned = ""
foreach ($Line in (Get-Content $ShaSumsPath)) {
  if ($Line -match '^\s*([0-9a-fA-F]{64})\s+\*?(.+)$') {
    $FileName = $Matches[2]
    if ($FileName.StartsWith('*')) {
      $FileName = $FileName.Substring(1)
    }
    if ($FileName -eq $ZipAssetName) {
      $ExpectedSigned = $Matches[1].ToLowerInvariant()
      break
    }
  }
}
if ([string]::IsNullOrWhiteSpace($ExpectedSigned)) {
  Fail "$ZipAssetName not found in signed SHA256SUMS."
}

$GotSigned = (Get-FileHash -Algorithm SHA256 $ZipPath).Hash.ToLowerInvariant()
if ($GotSigned -ne $ExpectedSigned) {
  Fail "signed SHA256SUMS mismatch. Expected $ExpectedSigned, got $GotSigned."
}
Write-Host "Signed SHA256SUMS check: OK"

Write-Host "Extracting into repo root: $RepoRoot"
Expand-Archive -Path $ZipPath -DestinationPath $RepoRoot -Force

$Required = @(
  "models\intent_macro\model.onnx",
  "models\distilbert-sst2\model.onnx",
  "models\toxic_quantized\model.onnx",
  "models\factcheck\model.onnx",
  "models\intent\model.onnx"
)

foreach ($Rel in $Required) {
  $Path = Join-Path $RepoRoot $Rel
  if (!(Test-Path $Path)) {
    Fail "Expected file not found after extraction: $Rel`nHint: the zip should contain a top-level 'models\\' directory."
  }
}

Write-Host "Done. Models are ready under: $(Join-Path $RepoRoot 'models')"

Remove-Item -Recurse -Force $TempDir
