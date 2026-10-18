param([switch]$Check, [switch]$Test, [switch]$Run)
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    $localCargo = Join-Path $PSScriptRoot '.tools\cargo\bin\cargo.exe'
    if (Test-Path -LiteralPath $localCargo) {
        $env:CARGO_HOME = Join-Path $PSScriptRoot '.tools\cargo'
        $env:RUSTUP_HOME = Join-Path $PSScriptRoot '.tools\rustup'
        $cargoExecutable = $localCargo
    } else {
        $cargoExecutable = (Get-Command cargo -ErrorAction Stop).Source
    }
    if ($Check) { & $cargoExecutable check --all-targets }
    elseif ($Test) { & $cargoExecutable test --release --workspace }
    elseif ($Run) { & $cargoExecutable run --release --bin cad }
    else { & $cargoExecutable build --release }
    if ($LASTEXITCODE -ne 0) { throw "Cargo failed with exit code $LASTEXITCODE" }
} finally { Pop-Location }
