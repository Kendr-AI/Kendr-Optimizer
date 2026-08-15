param(
    [string]$Version = $env:KENDR_VERSION,
    [string]$InstallDir = $env:KENDR_INSTALL_DIR
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$DefaultVersion = 'v0.1.4'
$Repository = 'Kendr-AI/Kendr-Optimizer'

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = $DefaultVersion
}
if ($Version -notmatch '^v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z][0-9A-Za-z-]*(?:\.[0-9A-Za-z][0-9A-Za-z-]*)*)?$') {
    throw "Invalid release version: $Version"
}

$InstallerTestDownload = $false
if (-not [string]::IsNullOrWhiteSpace($env:KENDR_DOWNLOAD_BASE_URL)) {
    if ($env:KENDR_INSTALLER_TEST_MODE -ne '1' -or
        $env:KENDR_ALLOW_INSECURE -ne '1' -or
        $env:KENDR_DOWNLOAD_BASE_URL -notmatch '^http://127\.0\.0\.1:[0-9]+/?$') {
        throw 'KENDR_DOWNLOAD_BASE_URL is restricted to numeric loopback installer tests.'
    }
    $InstallerTestDownload = $true
}

if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
    [System.Runtime.InteropServices.OSPlatform]::Windows
)) {
    throw 'This installer supports Windows only; use kendr-opt-installer.sh on macOS or Linux.'
}

$Architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
if ($Architecture -ne 'X64') {
    throw "Unsupported Windows architecture: $Architecture (this release supports x64)"
}
$Target = 'x86_64-pc-windows-msvc'
$Asset = "kendr-opt-$Target.zip"

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        throw 'LOCALAPPDATA is unavailable; set KENDR_INSTALL_DIR explicitly.'
    }
    $InstallDir = Join-Path $env:LOCALAPPDATA 'Kendr\bin'
}
$InstallDir = [System.IO.Path]::GetFullPath($InstallDir)

$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
    'kendr-opt-' + [System.Guid]::NewGuid().ToString('N')
)
$StagedPath = $null
$StagedReceipt = $null
$BinaryBackup = $null
$ReceiptBackup = $null
$KeepBinaryBackup = $false

function Test-GitHubCliAuthentication {
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        return $false
    }
    & gh auth status --hostname github.com *> $null
    return $LASTEXITCODE -eq 0
}

function Get-ReleaseAsset {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    if (-not $InstallerTestDownload -and (Test-GitHubCliAuthentication)) {
        & gh release download $Version `
            --repo "github.com/$Repository" `
            --pattern $Name `
            --output $Destination
        if ($LASTEXITCODE -ne 0) {
            throw "Could not download $Name with authenticated GitHub CLI."
        }
        return
    }

    $BaseUrl = if ($InstallerTestDownload) {
        $env:KENDR_DOWNLOAD_BASE_URL
    } else {
        "https://github.com/$Repository/releases/download/$Version"
    }

    [Net.ServicePointManager]::SecurityProtocol =
        [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $Uri = $BaseUrl.TrimEnd('/') + '/' + $Name
    try {
        Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $Destination
    } catch {
        throw "Could not download public release asset $Name. $($_.Exception.Message)"
    }
}

function ConvertTo-NormalizedPath {
    param([string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) {
        return $null
    }
    $Expanded = [Environment]::ExpandEnvironmentVariables($Value.Trim())
    try {
        return [System.IO.Path]::GetFullPath($Expanded).TrimEnd('\')
    } catch {
        return $Expanded.TrimEnd('\')
    }
}

function Add-UserPathEntry {
    param([string]$Directory)
    $NormalizedDirectory = ConvertTo-NormalizedPath $Directory
    $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $UserEntries = @($UserPath -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    $AlreadyPresent = $false
    foreach ($Entry in $UserEntries) {
        if ((ConvertTo-NormalizedPath $Entry) -ieq $NormalizedDirectory) {
            $AlreadyPresent = $true
            break
        }
    }
    if (-not $AlreadyPresent) {
        $NewUserPath = (@($UserEntries) + $Directory) -join ';'
        [Environment]::SetEnvironmentVariable('Path', $NewUserPath, 'User')
    }

    $CurrentEntries = @($env:Path -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if (-not ($CurrentEntries | Where-Object { (ConvertTo-NormalizedPath $_) -ieq $NormalizedDirectory })) {
        $env:Path = (@($CurrentEntries) + $Directory) -join ';'
    }
}

try {
    New-Item -ItemType Directory -Path $TempRoot | Out-Null
    $Archive = Join-Path $TempRoot $Asset
    $Checksums = Join-Path $TempRoot 'SHA256SUMS'
    Get-ReleaseAsset -Name $Asset -Destination $Archive
    Get-ReleaseAsset -Name 'SHA256SUMS' -Destination $Checksums

    $Pattern = '^([0-9a-fA-F]{64})  ' + [Regex]::Escape($Asset) + '$'
    $ChecksumMatches = @()
    foreach ($Line in Get-Content -LiteralPath $Checksums) {
        $Match = [Regex]::Match($Line, $Pattern)
        if ($Match.Success) {
            $ChecksumMatches += $Match.Groups[1].Value.ToLowerInvariant()
        }
    }
    if ($ChecksumMatches.Count -ne 1) {
        throw "SHA256SUMS must contain exactly one entry for $Asset."
    }
    $ActualChecksum = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($ActualChecksum -ne $ChecksumMatches[0]) {
        throw "SHA-256 mismatch for $Asset."
    }

    $ExtractDir = Join-Path $TempRoot 'extracted'
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $ArchiveRoot = "kendr-opt-$($Version.Substring(1))-$Target"
    $ExpectedEntries = @(
        "$ArchiveRoot/kendr-opt.exe",
        "$ArchiveRoot/CHANGELOG.md",
        "$ArchiveRoot/LICENSE",
        "$ArchiveRoot/NOTICE",
        "$ArchiveRoot/README.md",
        "$ArchiveRoot/RUST_STDLIB_LICENSES.html",
        "$ArchiveRoot/THIRD_PARTY_LICENSES.html"
    )
    $ArchiveHandle = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        $ActualEntries = @($ArchiveHandle.Entries | ForEach-Object { $_.FullName })
        if ($ActualEntries.Count -ne $ExpectedEntries.Count) {
            throw "$Asset has an unexpected archive layout."
        }
        foreach ($ExpectedEntry in $ExpectedEntries) {
            if (@($ActualEntries | Where-Object { $_ -ceq $ExpectedEntry }).Count -ne 1) {
                throw "$Asset has an unexpected archive layout."
            }
        }
    } finally {
        $ArchiveHandle.Dispose()
    }
    Expand-Archive -LiteralPath $Archive -DestinationPath $ExtractDir
    $Candidate = Join-Path $ExtractDir "$ArchiveRoot\kendr-opt.exe"
    if (-not (Test-Path -LiteralPath $Candidate -PathType Leaf)) {
        throw 'The archive does not contain the expected kendr-opt.exe binary.'
    }
    $CandidateItem = Get-Item -LiteralPath $Candidate
    if (($CandidateItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw 'The archive binary cannot be a reparse point.'
    }

    $ActualVersion = (& $Candidate --version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $ActualVersion -ne "kendr-opt $($Version.Substring(1))") {
        throw "Downloaded binary version mismatch: $ActualVersion"
    }
    $Engines = (& $Candidate engines --compact | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw 'Downloaded binary failed its engine smoke test.'
    }
    if ($Engines -notmatch '^\s*\[') {
        throw 'Downloaded binary engine output is not a JSON array.'
    }
    $ParsedEngines = @($Engines | ConvertFrom-Json)
    if ($ParsedEngines.Count -eq 0) {
        throw 'Downloaded binary returned an empty engine list.'
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $Destination = Join-Path $InstallDir 'kendr-opt.exe'
    $Receipt = Join-Path $InstallDir '.kendr-opt-install.json'
    $DestinationItem = Get-Item -LiteralPath $Destination -Force -ErrorAction SilentlyContinue
    if ($null -ne $DestinationItem) {
        if ($DestinationItem.PSIsContainer -or
            (($DestinationItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
            throw "Existing destination is not a regular file: $Destination"
        }
    }
    $ReceiptItem = Get-Item -LiteralPath $Receipt -Force -ErrorAction SilentlyContinue
    if ($null -ne $ReceiptItem) {
        if ($ReceiptItem.PSIsContainer -or
            (($ReceiptItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
            throw "Existing install receipt is not a regular file: $Receipt"
        }
    }

    $StagedPath = Join-Path $InstallDir (
        '.kendr-opt.' + [System.Guid]::NewGuid().ToString('N') + '.tmp.exe'
    )
    Copy-Item -LiteralPath $Candidate -Destination $StagedPath

    $StagedReceipt = Join-Path $InstallDir (
        '.kendr-opt-install.' + [System.Guid]::NewGuid().ToString('N') + '.tmp.json'
    )
    $ReceiptPayload = [ordered]@{
        schema_version = 'kendr.install/v1'
        repository = $Repository
        install_method = 'github-release'
        target = $Target
        version = $Version.Substring(1)
        channel = 'preview'
    }
    $ReceiptJson = $ReceiptPayload | ConvertTo-Json -Compress
    [IO.File]::WriteAllText(
        $StagedReceipt,
        $ReceiptJson + [Environment]::NewLine,
        [Text.UTF8Encoding]::new($false)
    )

    $DestinationExisted = $null -ne $DestinationItem
    $ReceiptExisted = $null -ne $ReceiptItem
    $BinaryInstalled = $false
    try {
        if ($DestinationExisted) {
            $BinaryBackup = Join-Path $InstallDir (
                '.kendr-opt.' + [System.Guid]::NewGuid().ToString('N') + '.backup.exe'
            )
            [IO.File]::Replace($StagedPath, $Destination, $BinaryBackup, $true)
            $StagedPath = $null
        } else {
            [IO.File]::Move($StagedPath, $Destination)
            $StagedPath = $null
        }
        $BinaryInstalled = $true

        if ($ReceiptExisted) {
            $ReceiptBackup = Join-Path $InstallDir (
                '.kendr-opt-install.' + [System.Guid]::NewGuid().ToString('N') + '.backup.json'
            )
            [IO.File]::Replace($StagedReceipt, $Receipt, $ReceiptBackup, $true)
            $StagedReceipt = $null
        } else {
            [IO.File]::Move($StagedReceipt, $Receipt)
            $StagedReceipt = $null
        }
    } catch {
        $InstallError = $_
        if (-not $BinaryInstalled) {
            throw $InstallError
        }
        if ($BinaryInstalled) {
            try {
                if ($DestinationExisted -and
                    $BinaryBackup -and
                    (Test-Path -LiteralPath $BinaryBackup -PathType Leaf)) {
                    $FailedBinary = Join-Path $InstallDir (
                        '.kendr-opt.' + [System.Guid]::NewGuid().ToString('N') + '.failed.exe'
                    )
                    [IO.File]::Replace($BinaryBackup, $Destination, $FailedBinary, $true)
                    $BinaryBackup = $null
                    Remove-Item -LiteralPath $FailedBinary -Force -ErrorAction SilentlyContinue
                } elseif (-not $DestinationExisted) {
                    Remove-Item -LiteralPath $Destination -Force -ErrorAction Stop
                }
            } catch {
                $KeepBinaryBackup = $true
                throw "Install receipt failed and the previous kendr-opt could not be restored. Rollback copy: $BinaryBackup. $($InstallError.Exception.Message)"
            }
        }
        throw "Install receipt failed; the binary installation was rolled back. $($InstallError.Exception.Message)"
    }

    if ($BinaryBackup) {
        Remove-Item -LiteralPath $BinaryBackup -Force -ErrorAction SilentlyContinue
        $BinaryBackup = $null
    }
    if ($ReceiptBackup) {
        Remove-Item -LiteralPath $ReceiptBackup -Force -ErrorAction SilentlyContinue
        $ReceiptBackup = $null
    }

    if ($env:KENDR_NO_MODIFY_PATH -notin @('1', 'true', 'TRUE')) {
        try {
            Add-UserPathEntry -Directory $InstallDir
        } catch {
            Write-Warning "Installed successfully, but could not update user PATH: $($_.Exception.Message)"
        }
    }

    Write-Output "Installed kendr-opt $($Version.Substring(1)) to $Destination"
    Write-Output 'Next: kendr-opt setup --list'
    $Resolved = Get-Command kendr-opt -ErrorAction SilentlyContinue
    if ($Resolved -and (ConvertTo-NormalizedPath $Resolved.Source) -ine (ConvertTo-NormalizedPath $Destination)) {
        Write-Warning "Another kendr-opt at $($Resolved.Source) currently appears earlier on PATH."
    }
} finally {
    if ($StagedPath) {
        Remove-Item -LiteralPath $StagedPath -Force -ErrorAction SilentlyContinue
    }
    if ($StagedReceipt) {
        Remove-Item -LiteralPath $StagedReceipt -Force -ErrorAction SilentlyContinue
    }
    if ($BinaryBackup -and -not $KeepBinaryBackup) {
        Remove-Item -LiteralPath $BinaryBackup -Force -ErrorAction SilentlyContinue
    }
    if ($ReceiptBackup) {
        Remove-Item -LiteralPath $ReceiptBackup -Force -ErrorAction SilentlyContinue
    }
    Remove-Item -LiteralPath $TempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
