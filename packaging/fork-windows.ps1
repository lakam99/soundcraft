param(
    [Parameter(Mandatory)][string]$BinaryDirectory,
    [Parameter(Mandatory)][string]$ChatPluginDirectory,
    [string]$Version = '0.17.2'
)
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$binaryRoot = (Resolve-Path -LiteralPath $BinaryDirectory).Path
$pluginRoot = (Resolve-Path -LiteralPath $ChatPluginDirectory).Path
& git -C $projectRoot diff --quiet HEAD --
if ($LASTEXITCODE -ne 0) { throw 'Commit the verified source before packaging' }
$sourceCommit = & git -C $projectRoot rev-parse HEAD
if ($LASTEXITCODE -ne 0) { throw 'Cannot identify package source' }
$pluginCommit = & git -C $pluginRoot rev-parse HEAD
if ($LASTEXITCODE -ne 0 -or $pluginCommit -ne 'cb38ab373dcf7ce3876418e0a0976af8d44c39c2') { throw 'Unexpected shared plugin license source' }
& git -C $pluginRoot diff --quiet HEAD --
if ($LASTEXITCODE -ne 0) { throw 'Shared plugin license source must be clean' }
$releaseRoot = Join-Path $projectRoot "dist/fork-$Version"
$stage = Join-Path $projectRoot ('target/fork-stage-' + [guid]::NewGuid().ToString('N'))
$zipPath = Join-Path $releaseRoot "soundcraft-$Version-windows-x64-portable.zip"
$sumsPath = Join-Path $releaseRoot 'SHA256SUMS.txt'
foreach ($outputFile in @($zipPath, $sumsPath)) {
    if (Test-Path -LiteralPath $outputFile) { throw "Release output already exists: $outputFile" }
}
[IO.Directory]::CreateDirectory($releaseRoot) | Out-Null
[IO.Directory]::CreateDirectory($stage) | Out-Null
foreach ($binary in @(@('soundcraft.exe', 2), @('soundcraft-cli.exe', 3))) {
    $path = Join-Path $binaryRoot $binary[0]
    $bytes = [IO.File]::ReadAllBytes($path)
    if ($bytes.Length -lt 128 -or $bytes[0] -ne 77 -or $bytes[1] -ne 90) { throw 'Invalid Windows executable' }
    $pe = [BitConverter]::ToInt32($bytes, 0x3c)
    if ($pe -lt 0 -or $pe -gt $bytes.Length - 96) { throw 'Invalid PE header' }
    if ([BitConverter]::ToUInt16($bytes, $pe + 4) -ne 0x8664 -or [BitConverter]::ToUInt16($bytes, $pe + 0x5c) -ne $binary[1]) {
        throw "Wrong architecture or subsystem: $path"
    }
    foreach ($encoding in @([Text.Encoding]::UTF8, [Text.Encoding]::Unicode)) {
        $text = $encoding.GetString($bytes)
        foreach ($privatePath in @($env:USERPROFILE, $env:USERPROFILE.Replace('\', '/'))) {
            if ($text.IndexOf($privatePath, [StringComparison]::OrdinalIgnoreCase) -ge 0) { throw 'Personal build path in executable; rebuild with path remapping' }
        }
    }
    Copy-Item -LiteralPath $path -Destination $stage
}
$product = (Get-Item -LiteralPath (Join-Path $stage 'soundcraft.exe')).VersionInfo
if ($product.ProductVersion -ne $Version -or $product.ProductName -ne 'SoundCraft') { throw 'Executable resource version mismatch' }
$cliVersion = & (Join-Path $stage 'soundcraft-cli.exe') --version
if ($LASTEXITCODE -ne 0 -or $cliVersion -notmatch [regex]::Escape($Version)) { throw 'CLI version mismatch' }
foreach ($document in @('README.md', 'LICENSE-MIT', 'LICENSE-APACHE', 'NOTICE', 'ATTRIBUTION.md')) {
    Copy-Item -LiteralPath (Join-Path $projectRoot $document) -Destination $stage
}
[IO.Directory]::CreateDirectory((Join-Path $stage 'docs')) | Out-Null
Copy-Item -LiteralPath (Join-Path $projectRoot 'docs/fork-guide.md'), (Join-Path $projectRoot 'docs/workspace-ux.md') -Destination (Join-Path $stage 'docs')
$pluginLicenses = Join-Path $stage 'licenses/craft-ai-chat'
[IO.Directory]::CreateDirectory($pluginLicenses) | Out-Null
foreach ($document in @('LICENSE-MIT', 'LICENSE-APACHE', 'NOTICE', 'README.md')) {
    Copy-Item -LiteralPath (Join-Path $pluginRoot $document) -Destination $pluginLicenses
}
Copy-Item -LiteralPath (Join-Path $pluginRoot 'third_party') -Destination $pluginLicenses -Recurse
Get-ChildItem -LiteralPath (Join-Path $projectRoot 'third_party') -Filter '*.txt' -File | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $stage 'licenses')
}
[IO.File]::WriteAllText((Join-Path $stage 'portable.txt'), "SoundCraft portable; keep SoundCraftData when upgrading.`n", [Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText((Join-Path $stage 'BUILD.txt'), "SoundCraft fork $Version`nSource: https://github.com/lakam99/soundcraft/commit/$sourceCommit`nAI plugin: $pluginCommit`nWindows x64 portable; unsigned.`n", [Text.UTF8Encoding]::new($false))
[IO.Compression.ZipFile]::CreateFromDirectory($stage, $zipPath, [IO.Compression.CompressionLevel]::Optimal, $false)
$hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText($sumsPath, ($hash + '  ' + [IO.Path]::GetFileName($zipPath) + "`n"), [Text.UTF8Encoding]::new($false))
Get-Item -LiteralPath $zipPath, $sumsPath | Select-Object Name, Length
