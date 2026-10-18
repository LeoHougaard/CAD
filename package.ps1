param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$workspaceRoot = [System.IO.Path]::GetFullPath($PSScriptRoot)
$workspacePrefix = $workspaceRoot.TrimEnd([char[]]'\/') + [System.IO.Path]::DirectorySeparatorChar
function Get-WorkspacePath([string]$RelativePath) {
    $absolute = [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot $RelativePath))
    if (-not $absolute.StartsWith($workspacePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Package path is outside the workspace: $absolute"
    }
    return $absolute
}
Push-Location $PSScriptRoot
try {
    if (-not $SkipBuild) { & './build.ps1' }
    $dist = Get-WorkspacePath 'dist'
    New-Item -ItemType Directory -Path $dist -Force | Out-Null
    $packageId = (Get-Date -Format 'yyyyMMdd-HHmmssfff') + '-' + [System.Guid]::NewGuid().ToString('N').Substring(0, 8)
    $archive = Get-WorkspacePath ".tools/previous-builds/package-$packageId"
    $binaries = @()
    try {
        # Stage every binary before moving an existing executable, including one in use.
        foreach ($name in @('CAD.exe', 'cad-agent.exe')) {
            $source = Get-WorkspacePath "target/release/$name"
            $binary = [pscustomobject]@{
                Target = Get-WorkspacePath "dist/$name"
                Staged = Get-WorkspacePath "dist/.$name.$packageId.staged"
                Previous = Get-WorkspacePath ".tools/previous-builds/package-$packageId/$name"
                Archived = $false
                Installed = $false
            }
            $binaries += $binary
            Copy-Item -LiteralPath $source -Destination $binary.Staged
            if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $binary.Staged -Algorithm SHA256).Hash) {
                throw "Staged executable differs from its source: $name"
            }
        }
        foreach ($binary in $binaries) {
            if (Test-Path -LiteralPath $binary.Target) {
                New-Item -ItemType Directory -Path $archive -Force | Out-Null
                Move-Item -LiteralPath $binary.Target -Destination $binary.Previous
                $binary.Archived = $true
            }
            Move-Item -LiteralPath $binary.Staged -Destination $binary.Target
            $binary.Installed = $true
        }
    } catch {
        $installError = $_
        for ($index = $binaries.Count - 1; $index -ge 0; $index--) {
            $binary = $binaries[$index]
            try {
                if ($binary.Installed) {
                    Move-Item -LiteralPath $binary.Target -Destination $binary.Staged
                }
                if ($binary.Archived) {
                    Move-Item -LiteralPath $binary.Previous -Destination $binary.Target
                }
            } catch {
                Write-Warning "Could not restore $($binary.Target). Previous executable remains at $($binary.Previous): $_"
            }
        }
        throw $installError
    } finally {
        foreach ($binary in $binaries) {
            if (Test-Path -LiteralPath $binary.Staged) {
                try { Remove-Item -LiteralPath $binary.Staged -ErrorAction Stop }
                catch { Write-Warning "Could not remove staging file $($binary.Staged): $_" }
            }
        }
    }
    foreach ($directory in @('plugins', 'examples')) {
        if (Test-Path -LiteralPath $directory) {
            New-Item -ItemType Directory -Path "dist/$directory" -Force | Out-Null
            Get-ChildItem -LiteralPath $directory -File | Where-Object { $_.Name -ne 'disabled.txt' } | Copy-Item -Destination "dist/$directory" -Force
        }
    }
    New-Item -ItemType Directory -Path (Join-Path $dist 'docs') -Force | Out-Null
    Get-ChildItem -LiteralPath 'docs' -Filter '*.md' -File | Copy-Item -Destination (Join-Path $dist 'docs') -Force
    Copy-Item -LiteralPath 'README.md', 'LICENSE' -Destination 'dist' -Force
    Write-Output "Portable build: $PSScriptRoot/dist/CAD.exe"
    Write-Output "Agent CLI: $PSScriptRoot/dist/cad-agent.exe"
} finally { Pop-Location }
