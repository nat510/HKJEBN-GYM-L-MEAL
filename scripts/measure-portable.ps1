param([string]$Executable = "$PSScriptRoot/../gymL.exe")
$ErrorActionPreference = 'Stop'
$exePath = (Resolve-Path -LiteralPath $Executable).Path
$dataPath = Join-Path (Split-Path -Parent $exePath) 'gymL_data'
$startedAt = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
$watch = [System.Diagnostics.Stopwatch]::StartNew()
$process = Start-Process -FilePath $exePath -WindowStyle Hidden -PassThru
$peakWorkingSet = 0L
$complete = $false
try {
    while ($watch.Elapsed.TotalSeconds -lt 180) {
        if ($process.HasExited) { throw "App exited before fetching completed: $($process.ExitCode)" }
        $process.Refresh()
        $peakWorkingSet = [Math]::Max($peakWorkingSet,$process.WorkingSet64)
        $completedFiles = 0
        foreach ($name in @('catalog.json','coupons.json','promotions.json')) {
            $path = Join-Path $dataPath $name
            if (Test-Path -LiteralPath $path) {
                try { $value = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json; if ($value.fetched_at -ge $startedAt) { $completedFiles++ } } catch { }
            }
        }
        if ($completedFiles -eq 3) { $complete = $true; break }
        Start-Sleep -Milliseconds 1000
    }
    if (-not $complete) { throw 'Fetch did not complete successfully within the smoke-test limit' }
    $fetchSeconds = $watch.Elapsed.TotalSeconds
    # Allow the completion notification and after-fetch cleanup to finish.
    Start-Sleep -Seconds 2
    $process.Refresh()
    $cpuBefore = $process.TotalProcessorTime.TotalMilliseconds
    $sample = [System.Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Seconds 10
    $process.Refresh()
    $cpuAfter = $process.TotalProcessorTime.TotalMilliseconds
    $result = [ordered]@{
        fetchCompleted = $true
        startupFetchSeconds = [Math]::Round($fetchSeconds,2)
        idleSampleSeconds = [Math]::Round($sample.Elapsed.TotalSeconds,2)
        idleCpuMilliseconds = [Math]::Round($cpuAfter-$cpuBefore,3)
        idleWorkingSetMB = [Math]::Round($process.WorkingSet64/1MB,2)
        idlePrivateMemoryMB = [Math]::Round($process.PrivateMemorySize64/1MB,2)
        sampledPeakWorkingSetMB = [Math]::Round($peakWorkingSet/1MB,2)
        idleThreads = $process.Threads.Count
        executableBytes = (Get-Item -LiteralPath $exePath).Length
    }
    $result | ConvertTo-Json
    $outputPath = Join-Path $PSScriptRoot '../tests/output/performance.json'
    [System.IO.File]::WriteAllText([System.IO.Path]::GetFullPath($outputPath),($result|ConvertTo-Json),[System.Text.UTF8Encoding]::new($false))
} finally {
    if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
    $process.Dispose()
}
