#requires -Version 7.0

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$FilePath,

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ArgumentList
)

$ErrorActionPreference = 'Stop'

$startInfo = [System.Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $FilePath
$startInfo.UseShellExecute = $false
foreach ($argument in $ArgumentList) {
    [void]$startInfo.ArgumentList.Add($argument)
}

$process = [System.Diagnostics.Process]::new()
$process.StartInfo = $startInfo

try {
    if (-not $process.Start()) {
        throw "failed to start '$FilePath'"
    }

    $process.WaitForExit()
    $exitCode = $process.ExitCode
    $peakWorkingSetBytes = $process.PeakWorkingSet64

    Write-Output 'resource_format=blcvoice-resource-evidence-v1'
    Write-Output 'resource_platform=Windows'
    Write-Output 'resource_metric=windows_peak_working_set_bytes'
    Write-Output "resource_value=$peakWorkingSetBytes"
    Write-Output 'resource_semantics=Windows PeakWorkingSetSize/PeakWorkingSet64; bytes'
    Write-Output "resource_command_exit_code=$exitCode"

    exit $exitCode
}
finally {
    $process.Dispose()
}
