$ErrorActionPreference = 'Stop'

$repository = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$forbiddenExtensions = @('.pfx', '.p12', '.pem', '.key', '.cer', '.cat', '.dll', '.exe', '.msi', '.zip', '.7z')
$forbiddenNames = @('target', 'dist', 'vdd_temp')

$tracked = @(git -C $repository ls-files)
if ($LASTEXITCODE -ne 0) { throw 'Could not enumerate tracked public files.' }
if ($tracked.Count -eq 0) { throw 'The public repository has no tracked files to verify.' }
$files = @($tracked | ForEach-Object { Get-Item -LiteralPath (Join-Path $repository $_) })

$forbiddenFiles = @($files | Where-Object {
    $forbiddenExtensions -contains $_.Extension.ToLowerInvariant() -or
    $forbiddenNames -contains $_.Directory.Name.ToLowerInvariant()
})
if ($forbiddenFiles.Count -gt 0) {
    $paths = $forbiddenFiles.FullName -join [Environment]::NewLine
    throw "Forbidden binary, credential or build artifact found:`n$paths"
}

$credentialPatterns = @(
    'api[_-]?key\s*=',
    'client[_-]?secret\s*=',
    'password\s*=',
    'BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY',
    '[A-Za-z]:\\(?:Users|github_project)\\',
    '/home/[^/]+/'
)
$verifierPath = (Resolve-Path $MyInvocation.MyCommand.Path).Path
$scannableFiles = @($files | Where-Object { $_.FullName -ne $verifierPath })
foreach ($pattern in $credentialPatterns) {
    $matches = @($scannableFiles | Select-String -Pattern $pattern -CaseSensitive:$false -ErrorAction SilentlyContinue)
    if ($matches.Count -gt 0) {
        throw "Possible credential matched '$pattern' in $($matches[0].Path)"
    }
}

$policy = Get-Content -LiteralPath (Join-Path $repository 'crates\product-policy\src\lib.rs') -Raw
if ($policy -notmatch 'edition:\s*ProductEdition::Lite' -or
    $policy -notmatch 'max_virtual_displays:\s*1') {
    throw 'The authoritative public product policy is not locked to Lite / one display.'
}

$serviceManifest = Get-Content -LiteralPath (Join-Path $repository 'crates\service\Cargo.toml') -Raw
if ($serviceManifest -match '(?m)^\s*pro\s*=') {
    throw 'A Pro service feature must not be present in the public repository.'
}

Write-Output "Public boundary verified: $($files.Count) files checked."
