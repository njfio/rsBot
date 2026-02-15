[CmdletBinding()]
param(
    [string]$Version = "",
    [string]$InstallDir = "",
    [string]$BinaryName = "tau-coding-agent",
    [switch]$Update,
    [switch]$Force,
    [switch]$DryRun,
    [switch]$NoVerify,
    [switch]$PrintTarget
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$AppName = "tau-coding-agent"
$RepoSlug = if ($env:TAU_RELEASE_REPO) { $env:TAU_RELEASE_REPO } else { "njfio/Tau" }
$ReleaseBaseUrl = if ($env:TAU_RELEASE_BASE_URL) { $env:TAU_RELEASE_BASE_URL } else { "https://github.com/$RepoSlug/releases/download" }
$LatestBaseUrl = if ($env:TAU_RELEASE_LATEST_URL) { $env:TAU_RELEASE_LATEST_URL } else { "https://github.com/$RepoSlug/releases/latest/download" }

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    if ($IsWindows) {
        $InstallDir = Join-Path $env:LOCALAPPDATA "Tau\bin"
    }
    else {
        $InstallDir = Join-Path $HOME ".local/bin"
    }
}

function Write-Reason {
    param(
        [string]$Level,
        [string]$ReasonCode,
        [string]$Message
    )
    $payload = [ordered]@{
        ts          = (Get-Date).ToUniversalTime().ToString("o")
        component   = "release-installer"
        level       = $Level
        reason_code = $ReasonCode
        message     = $Message
    }
    Write-Output ($payload | ConvertTo-Json -Compress)
}

function Fail-Reason {
    param(
        [string]$ReasonCode,
        [string]$Message
    )
    Write-Reason -Level "error" -ReasonCode $ReasonCode -Message $Message | Write-Error
    throw $Message
}

function Resolve-OsSlug {
    if ($env:TAU_INSTALL_TEST_OS) {
        $candidate = $env:TAU_INSTALL_TEST_OS.ToLowerInvariant()
    }
    elseif ($IsWindows) {
        $candidate = "windows"
    }
    elseif ($IsLinux) {
        $candidate = "linux"
    }
    elseif ($IsMacOS) {
        $candidate = "macos"
    }
    else {
        Fail-Reason -ReasonCode "unsupported_os" -Message "unsupported operating system"
    }

    switch ($candidate) {
        "windows" { return "windows" }
        "linux" { return "linux" }
        "macos" { return "macos" }
        default { Fail-Reason -ReasonCode "unsupported_os" -Message "unsupported operating system: $candidate" }
    }
}

function Resolve-ArchSlug {
    if ($env:TAU_INSTALL_TEST_ARCH) {
        $candidate = $env:TAU_INSTALL_TEST_ARCH.ToLowerInvariant()
    }
    else {
        $candidate = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
    }

    switch ($candidate) {
        { $_ -in @("x64", "x86_64", "amd64") } { return "amd64" }
        { $_ -in @("arm64", "aarch64") } { return "arm64" }
        default { Fail-Reason -ReasonCode "unsupported_arch" -Message "unsupported architecture: $candidate" }
    }
}

function Read-ChecksumHash {
    param([string]$ChecksumPath)
    $line = Get-Content -Path $ChecksumPath -TotalCount 1
    if ([string]::IsNullOrWhiteSpace($line)) {
        Fail-Reason -ReasonCode "checksum_manifest_invalid" -Message "checksum manifest is empty"
    }
    $parts = $line -split "\s+"
    $hash = $parts[0].ToLowerInvariant()
    if ($hash -notmatch "^[a-f0-9]{64}$") {
        Fail-Reason -ReasonCode "checksum_manifest_invalid" -Message "checksum manifest does not contain a valid SHA256"
    }
    return $hash
}

$OsSlug = Resolve-OsSlug
$ArchSlug = Resolve-ArchSlug
$Platform = "$OsSlug-$ArchSlug"
$ArchiveExt = if ($OsSlug -eq "windows") { "zip" } else { "tar.gz" }
$ArchiveName = "$AppName-$Platform.$ArchiveExt"
$ArchiveUrl = if ([string]::IsNullOrWhiteSpace($Version)) { "$LatestBaseUrl/$ArchiveName" } else { "$ReleaseBaseUrl/$Version/$ArchiveName" }
$ChecksumUrl = "$ArchiveUrl.sha256"

if ($PrintTarget) {
    Write-Output "platform=$Platform"
    Write-Output "archive=$ArchiveName"
    Write-Output "archive_url=$ArchiveUrl"
    Write-Output "checksum_url=$ChecksumUrl"
    exit 0
}

if ($DryRun) {
    Write-Reason -Level "info" -ReasonCode "dry_run" -Message "resolved install metadata only"
    Write-Output "platform=$Platform"
    Write-Output "archive_url=$ArchiveUrl"
    Write-Output "install_dir=$InstallDir"
    exit 0
}

$WorkDir = Join-Path ([System.IO.Path]::GetTempPath()) ("tau-install-" + [System.Guid]::NewGuid().ToString("N"))
$ExtractDir = Join-Path $WorkDir "extract"
$ArchivePath = Join-Path $WorkDir $ArchiveName
$ChecksumPath = Join-Path $WorkDir "$ArchiveName.sha256"
$DestinationName = if ($OsSlug -eq "windows" -and -not $BinaryName.EndsWith(".exe")) { "$BinaryName.exe" } else { $BinaryName }
$DestinationPath = Join-Path $InstallDir $DestinationName
$BackupPath = ""

New-Item -ItemType Directory -Path $WorkDir -Force | Out-Null
New-Item -ItemType Directory -Path $ExtractDir -Force | Out-Null

try {
    if ($Update -and -not (Test-Path -Path $DestinationPath -PathType Leaf)) {
        Fail-Reason -ReasonCode "update_target_missing" -Message "update requested but destination does not exist: $DestinationPath"
    }
    if (-not $Update -and (Test-Path -Path $DestinationPath -PathType Leaf) -and -not $Force) {
        Fail-Reason -ReasonCode "destination_exists" -Message "destination exists; rerun with -Force or -Update: $DestinationPath"
    }

    Write-Reason -Level "info" -ReasonCode "download_started" -Message "downloading release archive"
    Invoke-WebRequest -Uri $ArchiveUrl -OutFile $ArchivePath

    if (-not $NoVerify) {
        Write-Reason -Level "info" -ReasonCode "checksum_fetch_started" -Message "downloading checksum manifest"
        Invoke-WebRequest -Uri $ChecksumUrl -OutFile $ChecksumPath
        $expectedHash = Read-ChecksumHash -ChecksumPath $ChecksumPath
        $actualHash = (Get-FileHash -Path $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actualHash -ne $expectedHash) {
            Fail-Reason -ReasonCode "checksum_mismatch" -Message "archive checksum mismatch"
        }
        Write-Reason -Level "info" -ReasonCode "checksum_verified" -Message "checksum verification succeeded"
    }
    else {
        Write-Reason -Level "warn" -ReasonCode "checksum_verification_skipped" -Message "checksum verification disabled by operator"
    }

    if ($ArchiveExt -eq "zip") {
        Expand-Archive -Path $ArchivePath -DestinationPath $ExtractDir -Force
        $PackagedBinaryName = "$AppName-$Platform.exe"
    }
    else {
        tar -xzf $ArchivePath -C $ExtractDir
        $PackagedBinaryName = "$AppName-$Platform"
    }

    $SourceItem = Get-ChildItem -Path $ExtractDir -Recurse -File | Where-Object { $_.Name -eq $PackagedBinaryName } | Select-Object -First 1
    if ($null -eq $SourceItem) {
        Fail-Reason -ReasonCode "binary_not_found" -Message "packaged binary not found in archive: $PackagedBinaryName"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    if (Test-Path -Path $DestinationPath -PathType Leaf) {
        $BackupPath = "$DestinationPath.bak.$PID"
        Copy-Item -Path $DestinationPath -Destination $BackupPath -Force
    }

    try {
        Copy-Item -Path $SourceItem.FullName -Destination $DestinationPath -Force
        if ($OsSlug -ne "windows") {
            chmod +x $DestinationPath
        }
        & $DestinationPath --help | Out-Null
    }
    catch {
        if ($BackupPath -and (Test-Path -Path $BackupPath -PathType Leaf)) {
            Copy-Item -Path $BackupPath -Destination $DestinationPath -Force
        }
        Fail-Reason -ReasonCode "smoke_test_failed" -Message "installed binary failed --help smoke test"
    }
    finally {
        if ($BackupPath -and (Test-Path -Path $BackupPath -PathType Leaf)) {
            Remove-Item -Path $BackupPath -Force -ErrorAction SilentlyContinue
        }
    }

    if ($Update) {
        Write-Reason -Level "info" -ReasonCode "update_complete" -Message "update completed successfully"
    }
    else {
        Write-Reason -Level "info" -ReasonCode "install_complete" -Message "install completed successfully"
    }
    Write-Output "install_path=$DestinationPath"
}
finally {
    Remove-Item -Path $WorkDir -Recurse -Force -ErrorAction SilentlyContinue
}
