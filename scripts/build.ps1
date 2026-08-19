param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$BuildArgs
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$buildExitCode = 1
Push-Location -LiteralPath $repoRoot
try {
    & cargo mentat-build @BuildArgs
    $buildExitCode = $LASTEXITCODE
}
finally {
    Pop-Location
}
exit $buildExitCode
