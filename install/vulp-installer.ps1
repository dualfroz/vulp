# Installs the latest vulp release for Windows x86_64.
# Usage: irm https://raw.githubusercontent.com/dualfroz/vulp/main/install/vulp-installer.ps1 | iex
$ErrorActionPreference = "Stop"

$Repo = if ($env:VULP_REPO) { $env:VULP_REPO } else { "dualfroz/vulp" }
$InstallDir = if ($env:VULP_INSTALL_DIR) { $env:VULP_INSTALL_DIR } else { "$env:LOCALAPPDATA\vulp" }
$BinName = "vulp.exe"

Write-Host "Fetching latest vulp release info from github.com/$Repo..."
$release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
$asset = $release.assets | Where-Object { $_.name -like "*vulp-windows-x86_64*" } | Select-Object -First 1

if (-not $asset) {
    Write-Error "vulp-installer: couldn't find a windows-x86_64 asset in the latest release."
    exit 1
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$dest = Join-Path $InstallDir $BinName
Write-Host "Downloading $($asset.browser_download_url)..."
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $dest

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$InstallDir", "User")
    $env:Path = "$env:Path;$InstallDir"
    Write-Host "Added $InstallDir to your user PATH (open a new terminal for it to take effect everywhere)."
}

Write-Host "Installed vulp to $dest"
& $dest
