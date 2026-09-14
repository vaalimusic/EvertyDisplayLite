param(
    [string]$Version,
    [string]$InnoSetupPath,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repository = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$manifestPath = Join-Path $repository 'Cargo.toml'

if (-not $Version) {
    $manifest = Get-Content -LiteralPath $manifestPath -Raw
    $versionMatch = [regex]::Match($manifest, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"')
    if (-not $versionMatch.Success) {
        throw 'Could not read the workspace version from Cargo.toml.'
    }
    $Version = $versionMatch.Groups[1].Value
}
if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') {
    throw "Installer version must use MAJOR.MINOR.PATCH format, got '$Version'."
}

Push-Location $repository
try {
    & ./scripts/verify-public-tree.ps1
    if ($LASTEXITCODE -ne 0) { throw 'Public boundary verification failed.' }

    if (-not $SkipBuild) {
        & cargo build --workspace --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    }

    $releaseDirectory = Join-Path $repository 'target\release'
    $uiExecutable = Join-Path $releaseDirectory 'EvertyDisplay.exe'
    $serviceExecutable = Join-Path $releaseDirectory 'multitor-service.exe'
    foreach ($requiredBinary in @($uiExecutable, $serviceExecutable)) {
        if (-not (Test-Path -LiteralPath $requiredBinary -PathType Leaf)) {
            throw "Required release binary was not found: $requiredBinary"
        }
    }

    $payloadDirectory = Join-Path $repository 'artifacts\installer-payload'
    if (Test-Path -LiteralPath $payloadDirectory) {
        Remove-Item -LiteralPath $payloadDirectory -Recurse -Force
    }
    $driverDirectory = Join-Path $payloadDirectory 'driver\signed_x64'
    New-Item -ItemType Directory -Path $driverDirectory -Force | Out-Null

    Copy-Item -LiteralPath $uiExecutable -Destination $payloadDirectory
    Copy-Item -LiteralPath $serviceExecutable -Destination $payloadDirectory
    Copy-Item -LiteralPath (Join-Path $repository 'crates\ui\evertydisplay.ico') -Destination $payloadDirectory

    $driverVersion = '24.12.24'
    $driverArchiveSha256 = 'F93E7CE3D640C83C419B1B72D2C83B2D8E34D83E8B7BA11F3328A40D2DA82FA4'
    $driverSignerThumbprint = '3CF8CF26D8BA266C3A483AB7D26D4A818E317D76'
    $driverUrl = "https://github.com/VirtualDrivers/Virtual-Display-Driver/releases/download/$driverVersion/Signed-Driver-v$driverVersion-x64.zip"
    $downloadDirectory = New-Item -ItemType Directory -Path (Join-Path ([IO.Path]::GetTempPath()) ('evertydisplay-installer-' + [guid]::NewGuid().ToString('N')))

    try {
        $archivePath = Join-Path $downloadDirectory.FullName 'driver.zip'
        Invoke-WebRequest -Uri $driverUrl -OutFile $archivePath -UseBasicParsing
        $actualArchiveHash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
        if ($actualArchiveHash -ne $driverArchiveSha256) {
            throw "Driver archive SHA-256 mismatch. Expected $driverArchiveSha256, got $actualArchiveHash."
        }

        $expandedDirectory = Join-Path $downloadDirectory.FullName 'expanded'
        Expand-Archive -LiteralPath $archivePath -DestinationPath $expandedDirectory
        foreach ($driverFileName in @('mttvdd.cat', 'MttVDD.dll', 'MttVDD.inf')) {
            $driverFile = Join-Path $expandedDirectory $driverFileName
            if (-not (Test-Path -LiteralPath $driverFile -PathType Leaf)) {
                throw "The verified driver archive does not contain $driverFileName."
            }
            Copy-Item -LiteralPath $driverFile -Destination $driverDirectory
        }

        foreach ($signedFileName in @('mttvdd.cat', 'MttVDD.dll')) {
            $signedFile = Join-Path $driverDirectory $signedFileName
            $signature = Get-AuthenticodeSignature -LiteralPath $signedFile
            if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
                throw "Authenticode validation failed for ${signedFileName}: $($signature.StatusMessage)"
            }
            if (-not $signature.SignerCertificate -or $signature.SignerCertificate.Thumbprint -ne $driverSignerThumbprint) {
                throw "Unexpected Authenticode signer for $signedFileName."
            }
        }
    }
    finally {
        Remove-Item -LiteralPath $downloadDirectory.FullName -Recurse -Force -ErrorAction SilentlyContinue
    }

    Copy-Item -LiteralPath (Join-Path $repository 'installer\vdd_settings.xml') -Destination $driverDirectory

    if (-not $InnoSetupPath) {
        $isccCommand = Get-Command ISCC.exe -ErrorAction SilentlyContinue
        if ($isccCommand) { $InnoSetupPath = $isccCommand.Source }
    }
    if (-not $InnoSetupPath) {
        $defaultCompiler = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'
        if (Test-Path -LiteralPath $defaultCompiler -PathType Leaf) { $InnoSetupPath = $defaultCompiler }
    }
    if (-not $InnoSetupPath -or -not (Test-Path -LiteralPath $InnoSetupPath -PathType Leaf)) {
        throw 'Inno Setup 6 compiler was not found. Install it or pass -InnoSetupPath.'
    }

    $distDirectory = Join-Path $repository 'dist'
    New-Item -ItemType Directory -Path $distDirectory -Force | Out-Null
    & $InnoSetupPath "/DMyAppVersion=$Version" (Join-Path $repository 'installer\EvertyDisplayLiteSetup.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Inno Setup compilation failed.' }

    $installerPath = Join-Path $distDirectory "EvertyDisplay-Lite-Setup-$Version.exe"
    if (-not (Test-Path -LiteralPath $installerPath -PathType Leaf)) {
        throw "Inno Setup completed without the expected artifact: $installerPath"
    }
    Write-Output "Installer created: $installerPath"
}
finally {
    Pop-Location
}
