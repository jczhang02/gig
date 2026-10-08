# Windows build steps for /partjob build: run from the project root by buildspec.yml (AWS CodeBuild,
# Windows Server Core 2022) and by the skill's scripts/vm-build.sh (the local win11 VM).
# Same work as the Windows job of the GitHub Actions workflow: install uv and Python 3.11, test, package, smoke test.
# The artifacts are the regular files directly in dist/. A onedir build zips its folder into dist/ with
# `tar.exe -a -c -f dist\PROJECT_NAME-windows.zip -C dist PROJECT_NAME`; never Compress-Archive, which in
# Windows PowerShell 5.1 writes backslash paths that unpack wrongly outside Windows.
# Keep this file ASCII. Non-ASCII text needs a UTF-8 BOM for Windows PowerShell 5.1.

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$env:PYTHONUTF8 = "1"
$env:PYTHONIOENCODING = "utf-8"
[Console]::OutputEncoding = [Text.Encoding]::UTF8  # over ssh the VM console is otherwise GBK

# PowerShell does not stop when an external command fails; check the exit code ourselves.
function Step([string]$Name, [scriptblock]$Block) {
    Write-Host "==== $Name"
    $global:LASTEXITCODE = 0  # pure PowerShell steps never set it; reset before the check
    & $Block
    if ($LASTEXITCODE -ne 0) { throw "$Name failed, exit code $LASTEXITCODE" }
}

$env:UV_INSTALL_DIR = "C:\uv"
$env:UV_NO_MODIFY_PATH = "1"
Step "Install uv" { if (-not (Test-Path C:\uv\uv.exe)) { Invoke-RestMethod https://astral.sh/uv/install.ps1 | Invoke-Expression } }
$env:Path = "C:\uv;$env:Path"

Step "Install Python 3.11" { uv python install 3.11 }
Step "Install dependencies" { uv sync }
Step "Tests" { uv run python -m pytest tests }
Step "Package" {
    uv run pyinstaller --onefile --console --noconfirm --clean `
        --name PROJECT_NAME --paths src packaging/entry.py
}
Step "Smoke test --version" { .\dist\PROJECT_NAME.exe --version }
Step "Smoke test --help" { .\dist\PROJECT_NAME.exe --help }
