# Compile Cryptonaute en release et produit l'installeur Windows (NSIS) dans dist\.
# Prérequis : Rust (MSVC) et NSIS 3 (makensis), par exemple : winget install NSIS.NSIS
# Usage : pwsh scripts/build.ps1
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

# Garantit que cargo est trouvable (installation par rustup).
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargoBin) { $env:Path = "$cargoBin;$env:Path" }
if (-not (Test-Path (Join-Path $root 'third_party\wireguard-nt\wireguard.dll'))) {
    & (Join-Path $PSScriptRoot 'fetch-wireguard-nt.ps1')
}

Push-Location $root
try {
    cargo build --release -p cryptonaute -p cryptonaute-service
    if ($LASTEXITCODE -ne 0) { throw "échec de la compilation" }
}
finally { Pop-Location }

$version = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$candidates = @(
    (Get-Command makensis -ErrorAction SilentlyContinue).Source,
    "${env:ProgramFiles(x86)}\NSIS\makensis.exe",
    "$env:ProgramFiles\NSIS\makensis.exe",
    "$env:LOCALAPPDATA\tauri\NSIS\makensis.exe"
)
$makensis = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $makensis) { throw "NSIS (makensis) est introuvable : installez-le avec « winget install NSIS.NSIS »." }

$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$setup = Join-Path $dist "Cryptonaute_${version}_x64-setup.exe"
& $makensis /V2 "/DVERSION=$version" "/DROOT=$root" "/DOUTFILE=$setup" (Join-Path $root 'packaging\windows\installer.nsi')
if ($LASTEXITCODE -ne 0) { throw "échec de la construction de l'installeur" }
Write-Host "Installeur prêt : $setup" -ForegroundColor Green
