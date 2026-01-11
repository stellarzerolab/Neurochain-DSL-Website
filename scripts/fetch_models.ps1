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

Write-Host "Downloading model pack..."
Write-Host "  url: $Url"
Invoke-WebRequest -Uri $Url -OutFile $ZipPath -UseBasicParsing

if (![string]::IsNullOrWhiteSpace($Sha) -and $Sha -ne "TODO") {
  Write-Host "Verifying SHA256..."
  $Got = (Get-FileHash -Algorithm SHA256 $ZipPath).Hash.ToLowerInvariant()
  $Expected = $Sha.ToLowerInvariant()
  if ($Got -ne $Expected) {
    Fail "SHA256 mismatch. Expected $Expected, got $Got."
  }
} else {
  Write-Host "WARN: models_zip_sha256 is not set (or TODO); skipping checksum verification." -ForegroundColor Yellow
}

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

