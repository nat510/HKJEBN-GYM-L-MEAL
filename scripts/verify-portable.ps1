param([string]$Executable = "$PSScriptRoot/../gymL.exe")
$ErrorActionPreference = 'Stop'
$exePath = (Resolve-Path -LiteralPath $Executable).Path
$bytes = [System.IO.File]::ReadAllBytes($exePath)
$pe = [BitConverter]::ToInt32($bytes,0x3c)
if ([System.Text.Encoding]::ASCII.GetString($bytes,$pe,4) -ne "PE`0`0") { throw 'Not a PE executable' }
$machine = [BitConverter]::ToUInt16($bytes,$pe+4)
$count = [BitConverter]::ToUInt16($bytes,$pe+6)
$optionalSize = [BitConverter]::ToUInt16($bytes,$pe+20)
$optional = $pe+24
$magic = [BitConverter]::ToUInt16($bytes,$optional)
$subsystem = [BitConverter]::ToUInt16($bytes,$optional+68)
if ($machine -ne 0x8664 -or $magic -ne 0x20b -or $subsystem -ne 2) { throw 'Expected Windows x64 GUI executable' }
$sections = @()
for ($i=0; $i -lt $count; $i++) {
    $offset = $optional+$optionalSize+40*$i
    $sections += @{ Address=[BitConverter]::ToUInt32($bytes,$offset+12); Size=[Math]::Max([BitConverter]::ToUInt32($bytes,$offset+8),[BitConverter]::ToUInt32($bytes,$offset+16)); Raw=[BitConverter]::ToUInt32($bytes,$offset+20) }
}
function Rva-Offset([uint32]$address) {
    foreach ($section in $sections) { if ($address -ge $section.Address -and $address -lt $section.Address+$section.Size) { return [int]($address-$section.Address+$section.Raw) } }
    throw 'Invalid PE RVA'
}
function Read-Ascii([int]$offset) {
    $end = $offset
    while ($end -lt $bytes.Length -and $bytes[$end] -ne 0) { $end++ }
    [System.Text.Encoding]::ASCII.GetString($bytes,$offset,$end-$offset)
}
$imports = @()
$importRva = [BitConverter]::ToUInt32($bytes,$optional+120)
$importOffset = Rva-Offset $importRva
for ($i=0; $i -lt 256; $i++) {
    $nameRva = [BitConverter]::ToUInt32($bytes,$importOffset+20*$i+12)
    if ($nameRva -eq 0) { break }
    $imports += Read-Ascii (Rva-Offset $nameRva)
}
if (($imports -join ',') -match 'WebView|vcruntime|msvcp|d3d|dxgi') { throw 'Unexpected renderer/runtime import' }
$resourceRva = [BitConverter]::ToUInt32($bytes,$optional+128)
$resourceOffset = Rva-Offset $resourceRva
$named = [BitConverter]::ToUInt16($bytes,$resourceOffset+12)
$ids = [BitConverter]::ToUInt16($bytes,$resourceOffset+14)
$resourceTypes = @()
for ($i=$named; $i -lt $named+$ids; $i++) { $resourceTypes += [BitConverter]::ToUInt32($bytes,$resourceOffset+16+$i*8) }
foreach ($id in @(3,14,24)) { if ($resourceTypes -notcontains $id) { throw "Missing resource type $id (icon/group/manifest)" } }
$utf8 = [System.Text.Encoding]::UTF8.GetString($bytes)
$utf16 = [System.Text.Encoding]::Unicode.GetString($bytes)
$needles = @([Environment]::UserName, 'nat510','workers.dev','pages.dev','cloudflare','github.com/nat510')
foreach ($needle in $needles) { if ($needle.Length -ge 3 -and ($utf8.Contains($needle,[StringComparison]::OrdinalIgnoreCase) -or $utf16.Contains($needle,[StringComparison]::OrdinalIgnoreCase))) { throw "Personal/hosting identifier embedded in executable: $needle" } }
$zipPath = Join-Path (Split-Path -Parent $exePath) 'dist/gymL-portable-windows-x64.zip'
Add-Type -AssemblyName System.IO.Compression
$archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
try {
    $entries = @($archive.Entries | ForEach-Object FullName)
    if (@($entries | Where-Object { $_ -notin @('gymL.exe','README.txt','LICENSE','THIRD_PARTY_LICENSES.txt','gymL_data/') }).Count -ne 0) { throw 'Unexpected file in distribution' }
    if ($entries.Count -ne 5) { throw 'Missing portable package files' }
    $licenseReader = [System.IO.StreamReader]::new($archive.GetEntry('LICENSE').Open())
    try {
        if ($licenseReader.ReadToEnd() -ne [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot '../LICENSE'))) { throw 'Packaged application license differs from source license' }
    } finally { $licenseReader.Dispose() }
} finally { $archive.Dispose() }
$result = [ordered]@{ architecture='x64'; subsystem='Windows GUI'; resources=$resourceTypes; dllImports=$imports; personalIdentifierScan='pass'; zipEntries=$entries; executableBytes=$bytes.Length; sha256=(Get-FileHash -LiteralPath $exePath -Algorithm SHA256).Hash }
$result | ConvertTo-Json
$outputPath = Join-Path $PSScriptRoot '../tests/output/portable-verification.json'
[System.IO.File]::WriteAllText([System.IO.Path]::GetFullPath($outputPath),($result|ConvertTo-Json),[System.Text.UTF8Encoding]::new($false))
