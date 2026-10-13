param([switch]$SkipChecks)
$ErrorActionPreference = 'Stop'
$taskRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location $taskRoot
$previousFlags = $env:CARGO_ENCODED_RUSTFLAGS
try {
    if (-not $SkipChecks) {
        cargo fmt --check
        if ($LASTEXITCODE -ne 0) { throw 'Formatting check failed' }
        cargo test
        if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
        cargo clippy --all-targets -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
    }
    $env:CARGO_ENCODED_RUSTFLAGS = @('-C','target-feature=+crt-static', '--remap-path-prefix', "$taskRoot=.", '--remap-path-prefix', "$env:USERPROFILE=build") -join [char]31
    cargo build --release --locked --bin gymL
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    New-Item -ItemType Directory -Force -Path 'dist/gymL','gymL_data','dist/gymL/gymL_data' | Out-Null
    Copy-Item -LiteralPath 'target/release/gymL.exe' -Destination 'gymL.exe' -Force
    Copy-Item -LiteralPath 'gymL.exe' -Destination 'dist/gymL/gymL.exe' -Force
    Copy-Item -LiteralPath 'README-portable.txt' -Destination 'dist/gymL/README.txt' -Force
    Copy-Item -LiteralPath 'LICENSE' -Destination 'dist/gymL/LICENSE' -Force
    ./scripts/licenses.ps1 -OutputFile 'dist/gymL/THIRD_PARTY_LICENSES.txt'
    Add-Type -AssemblyName System.IO.Compression
    $zipPath = Join-Path $taskRoot 'dist/gymL-portable-windows-x64.zip'
    $stream = [System.IO.File]::Create($zipPath)
    $archive = [System.IO.Compression.ZipArchive]::new($stream,[System.IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($name in @('gymL.exe','README.txt','LICENSE','THIRD_PARTY_LICENSES.txt')) {
            $entry = $archive.CreateEntry($name,[System.IO.Compression.CompressionLevel]::Optimal)
            $destination = $entry.Open()
            $source = [System.IO.File]::OpenRead((Join-Path $taskRoot "dist/gymL/$name"))
            try { $source.CopyTo($destination) } finally { $source.Dispose(); $destination.Dispose() }
        }
        $null = $archive.CreateEntry('gymL_data/')
    } finally { $archive.Dispose(); $stream.Dispose() }
    Get-Item -LiteralPath 'gymL.exe','dist/gymL-portable-windows-x64.zip' | Select-Object Name,Length
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $previousFlags
    Pop-Location
}
