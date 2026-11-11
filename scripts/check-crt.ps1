# File: check-crt.ps1
# Purpose: Verify the five unmodified Microsoft x64 app-local runtime inputs, never install them.
# State: explicit licensed Visual Studio VC/Redist source only; signatures/company and SHA evidence.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Directory)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Get-Item -LiteralPath $Directory
if (-not $root.PSIsContainer -or $root.Attributes -band [IO.FileAttributes]::ReparsePoint -or
    $root.FullName -notmatch '[\\/]VC[\\/]Redist[\\/]MSVC[\\/]\d+\.\d+\.\d+[\\/]x64[\\/]Microsoft\.VC\d+\.CRT$') {
    throw 'Use an unmodified licensed Visual Studio x64 redistributable source.'
}
$receipts = [ordered]@{}
foreach ($name in @('msvcp140.dll','msvcp140_1.dll','msvcp140_2.dll','vcruntime140.dll','vcruntime140_1.dll')) {
    $file = Get-Item -LiteralPath (Join-Path $root.FullName $name)
    if ($file.PSIsContainer -or $file.Attributes -band [IO.FileAttributes]::ReparsePoint -or
        $file.Length -gt 4194304 -or $file.VersionInfo.CompanyName -cne 'Microsoft Corporation') {
        throw 'Microsoft runtime file metadata differs.'
    }
    $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') {
        throw 'Microsoft runtime Authenticode verification failed.'
    }
    $receipts[$name]=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
}
$receipts | ConvertTo-Json -Compress
