# Met à jour third_party\wireguard-nt\wireguard.dll (pilote noyau officiel,
# signé par WireGuard LLC) pour l'architecture courante, avec sa licence.
# La DLL est déjà fournie dans le dépôt : ce script ne sert qu'à changer de version.
# Usage : pwsh scripts/fetch-wireguard-nt.ps1 [-Version 0.10.1]
param([string]$Version = '0.10.1')
$ErrorActionPreference = 'Stop'

$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$out = Join-Path $root 'third_party\wireguard-nt'
$arch = switch ($env:PROCESSOR_ARCHITECTURE) { 'ARM64' { 'arm64' } 'x86' { 'x86' } default { 'amd64' } }
$url = "https://download.wireguard.com/wireguard-nt/wireguard-nt-$Version.zip"
$tmp = Join-Path ([IO.Path]::GetTempPath()) "wireguard-nt-$([guid]::NewGuid())"
$zip = "$tmp.zip"

try {
    Write-Host "Téléchargement de $url"
    Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
    Expand-Archive -Path $zip -DestinationPath $tmp
    $dll = Join-Path $tmp "wireguard-nt\bin\$arch\wireguard.dll"
    if (-not (Test-Path $dll)) { throw "wireguard.dll introuvable pour l'architecture $arch" }

    # Vérifie la signature Authenticode avant toute utilisation.
    $sig = Get-AuthenticodeSignature $dll
    if ($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch 'O=WireGuard LLC') {
        throw "Signature de wireguard.dll invalide : $($sig.Status) / $($sig.SignerCertificate.Subject)"
    }
    New-Item -ItemType Directory -Force $out | Out-Null
    Copy-Item $dll, (Join-Path $tmp 'wireguard-nt\LICENSE.txt') $out -Force
    Write-Host "OK : third_party\wireguard-nt\wireguard.dll $Version ($arch), signature valide"
}
finally {
    Remove-Item $zip, $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
