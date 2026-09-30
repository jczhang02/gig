# Build steps on AWS CodeBuild (Windows Server Core 2022), called from buildspec.yml.
# Same work as the Windows job of the GitHub Actions workflow: install uv and Python 3.11, test, package, smoke test.
# Keep this file ASCII. Non-ASCII text needs a UTF-8 BOM for Windows PowerShell 5.1.

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$env:PYTHONUTF8 = "1"
$env:PYTHONIOENCODING = "utf-8"

# PowerShell does not stop when an external command fails; check the exit code ourselves.
function Step([string]$Name, [scriptblock]$Block) {
    Write-Host "==== $Name"
    $global:LASTEXITCODE = 0  # pure PowerShell steps never set it; reset before the check
    & $Block
    if ($LASTEXITCODE -ne 0) { throw "$Name failed, exit code $LASTEXITCODE" }
}

$env:UV_INSTALL_DIR = "C:\uv"
$env:UV_NO_MODIFY_PATH = "1"
Step "Install uv" { Invoke-RestMethod https://astral.sh/uv/install.ps1 | Invoke-Expression }
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
