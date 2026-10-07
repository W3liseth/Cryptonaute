# Compile Cryptonaute en release et produit l'installeur Windows (NSIS) dans dist\.
# Prérequis : Rust (MSVC) et Node.js (pour la CLI Tauri, récupérée via npx).
# Usage : pwsh scripts/build.ps1
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')

# Garantit que node et cargo sont trouvables par les sous-processus (shims npx).
foreach ($dir in "$env:ProgramFiles\nodejs", "$env:USERPROFILE\.cargo\bin") {
    if (Test-Path $dir) { $env:Path = "$dir;$env:Path" }
}
if (-not (Test-Path (Join-Path $root 'third_party\wireguard-nt\wireguard.dll'))) {
    & (Join-Path $PSScriptRoot 'fetch-wireguard-nt.ps1')
}

Push-Location (Join-Path $root 'crates\app')
try {
    # Compile le service (beforeBuildCommand), l'application, puis assemble l'installeur.
    npx --yes '@tauri-apps/cli@2' build
    if ($LASTEXITCODE -ne 0) { throw "échec de la construction de l'installeur" }
}
finally { Pop-Location }

$setup = Get-ChildItem (Join-Path $root 'target\release\bundle\nsis\*-setup.exe') |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
Copy-Item $setup.FullName $dist -Force
Write-Host "Installeur prêt : $(Join-Path $dist $setup.Name)" -ForegroundColor Green
