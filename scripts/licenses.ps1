param([string]$OutputFile)
$ErrorActionPreference = 'Stop'
$metadataText = cargo metadata --format-version 1 --locked
if ($LASTEXITCODE -ne 0) { throw 'Could not read dependency metadata' }
$metadata = $metadataText | ConvertFrom-Json
$output = [System.Text.StringBuilder]::new()
$null = $output.AppendLine('gymL third-party dependency notices')
$null = $output.AppendLine('Dependencies and tools used to build this distribution; project is Rust with native Windows controls.')
foreach ($package in ($metadata.packages | Where-Object { $_.name -ne 'gyml' } | Sort-Object name,version)) {
    $null = $output.AppendLine("`n=== $($package.name) $($package.version) ===")
    $null = $output.AppendLine("License: $($package.license)")
    $directory = Split-Path -Parent $package.manifest_path
    $files = Get-ChildItem -LiteralPath $directory -File | Where-Object { $_.Name -match '^(LICENSE|COPYING|NOTICE)([-._].*)?$' }
    if ($package.license_file) {
        $extra = Join-Path $directory $package.license_file
        if (Test-Path -LiteralPath $extra) { $files = @($files) + (Get-Item -LiteralPath $extra) }
    }
    foreach ($file in ($files | Sort-Object FullName -Unique)) {
        $null = $output.AppendLine("`n[$($file.Name)]")
        $null = $output.AppendLine([System.IO.File]::ReadAllText($file.FullName))
    }
}
[System.IO.File]::WriteAllText([System.IO.Path]::GetFullPath($OutputFile),$output.ToString(),[System.Text.UTF8Encoding]::new($false))
