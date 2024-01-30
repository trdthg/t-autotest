Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repo = "trdthg/t-autotest"

Write-Host ">>> checking latest tag..."
$response = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest"
$tag = $response.tag_name
Write-Host "<<< latest tag: $tag"

Write-Host ">>> prepare dir"
$folder = "$env:USERPROFILE\.autotest"
if (-not (Test-Path -Path $folder)) {
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
}
Write-Host "<<< prepare success"

Write-Host ">>> downloading zip..."
Set-Location $folder
$zipName = "autotest-windows.zip"
Invoke-WebRequest -Uri "https://github.com/trdthg/t-autotest/releases/download/$tag/$zipName" -OutFile "$zipName"
Write-Host "<<< download success"

Write-Host ">>> extracting..."
Expand-Archive -Path "$zipName" -DestinationPath $folder -Force
Write-Host "<<< extract success"

Set-Location $env:USERPROFILE

Write-Host ">>> setting env..."
$envPath = [Environment]::GetEnvironmentVariable("PATH", "User")
[Environment]::SetEnvironmentVariable("PATH", "$envPath;$folder", "User")
Write-Host "<<< done!, you can try with 'autotest -v'"
