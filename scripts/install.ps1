# Install track from a GitHub release archive.
#
# Usage:
#   irm https://raw.githubusercontent.com/OrekGames/track-cli/main/scripts/install.ps1 | iex
#   $env:TRACK_VERSION = "1.15.1"; irm https://raw.githubusercontent.com/OrekGames/track-cli/v1.15.1/scripts/install.ps1 | iex
#
# Environment:
#   TRACK_VERSION      Optional release version, with or without a leading "v".
#   TRACK_INSTALL_DIR  Optional install directory. Defaults to $env:LOCALAPPDATA\Programs\track.
#   TRACK_SKIP_PATH    Set to 1 to skip user PATH changes.

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch {
    # PowerShell Core on non-Windows platforms may not expose ServicePointManager.
}

$Repo = "OrekGames/track-cli"
$GitHubApiUrl = "https://api.github.com/repos/$Repo"
$GitHubReleaseUrl = "https://github.com/$Repo/releases/download"

function Write-Step {
    param([string] $Message)
    Write-Host $Message
}

function Fail {
    param([string] $Message)
    throw "track installer: $Message"
}

function Normalize-TrackVersion {
    param([string] $InputVersion)

    if ([string]::IsNullOrWhiteSpace($InputVersion)) {
        Fail "TRACK_VERSION cannot be empty"
    }

    $Version = $InputVersion.Trim()
    if ($Version.StartsWith("v")) {
        $Version = $Version.Substring(1)
    }

    if ([string]::IsNullOrWhiteSpace($Version)) {
        Fail "TRACK_VERSION cannot be empty"
    }

    return $Version
}

function Get-LatestTrackVersion {
    $Headers = @{ "User-Agent" = "track-installer" }
    $Release = Invoke-RestMethod -Uri "$GitHubApiUrl/releases/latest" -Headers $Headers
    if (-not $Release.tag_name) {
        Fail "could not determine the latest release version"
    }

    $LatestVersion = Normalize-TrackVersion -InputVersion ([string] $Release.tag_name)
    return $LatestVersion
}

function Get-TrackTarget {
    $Arch = $env:PROCESSOR_ARCHITEW6432
    if ([string]::IsNullOrWhiteSpace($Arch)) {
        $Arch = $env:PROCESSOR_ARCHITECTURE
    }

    switch -Regex ($Arch) {
        "^(AMD64|x64)$" { return "x86_64-pc-windows-msvc" }
        "^(ARM64|AARCH64)$" { return "aarch64-pc-windows-msvc" }
        default { Fail "unsupported Windows architecture: $Arch" }
    }
}

function Download-File {
    param(
        [string] $Url,
        [string] $Destination
    )

    Invoke-WebRequest -Uri $Url -OutFile $Destination -UseBasicParsing
}

function Get-ExpectedChecksum {
    param(
        [string] $ChecksumsPath,
        [string] $ArchiveName
    )

    $EscapedName = [regex]::Escape($ArchiveName)
    foreach ($Line in Get-Content -Path $ChecksumsPath) {
        if ($Line -match "^\s*([a-fA-F0-9]{64})\s+\*?$EscapedName\s*$") {
            return $Matches[1].ToLowerInvariant()
        }
    }

    Fail "checksum not found for $ArchiveName"
}

function Verify-Checksum {
    param(
        [string] $ChecksumsPath,
        [string] $ArchivePath,
        [string] $ArchiveName
    )

    $Expected = Get-ExpectedChecksum -ChecksumsPath $ChecksumsPath -ArchiveName $ArchiveName
    $Actual = (Get-FileHash -Algorithm SHA256 -Path $ArchivePath).Hash.ToLowerInvariant()

    if ($Expected -ne $Actual) {
        Fail "checksum verification failed for $ArchiveName"
    }
}

function Add-InstallDirToUserPath {
    param([string] $Directory)

    $CurrentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $Entries = @()
    if (-not [string]::IsNullOrWhiteSpace($CurrentPath)) {
        $Entries = $CurrentPath -split ";" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    }

    $NormalizedDirectory = $Directory.TrimEnd("\")
    $AlreadyPresent = $Entries | Where-Object { $_.TrimEnd("\") -ieq $NormalizedDirectory } | Select-Object -First 1
    if ($AlreadyPresent) {
        Write-Step "User PATH already contains $Directory"
        return
    }

    if ([string]::IsNullOrWhiteSpace($CurrentPath)) {
        $NewPath = $Directory
    } else {
        $NewPath = "$CurrentPath;$Directory"
    }

    [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")

    $ProcessEntries = $env:Path -split ";" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    $ProcessAlreadyPresent = $ProcessEntries | Where-Object { $_.TrimEnd("\") -ieq $NormalizedDirectory } | Select-Object -First 1
    if (-not $ProcessAlreadyPresent) {
        $env:Path = "$env:Path;$Directory"
    }

    Write-Step "Added $Directory to the user PATH."
}

if ($env:TRACK_VERSION) {
    $Version = Normalize-TrackVersion -InputVersion $env:TRACK_VERSION
} else {
    $Version = Get-LatestTrackVersion
}

$Tag = "v$Version"
$Target = Get-TrackTarget
$ArchiveName = "track-$Version-$Target.zip"
$PackageName = "track-$Version-$Target"

if ($env:TRACK_INSTALL_DIR) {
    $InstallDir = $env:TRACK_INSTALL_DIR
} else {
    $LocalAppData = $env:LOCALAPPDATA
    if ([string]::IsNullOrWhiteSpace($LocalAppData)) {
        $LocalAppData = Join-Path $HOME "AppData\Local"
    }
    $InstallDir = Join-Path $LocalAppData "Programs\track"
}

$InstallDir = [System.IO.Path]::GetFullPath($InstallDir)
$CompletionsDir = Join-Path $InstallDir "completions"
$TrackExe = Join-Path $InstallDir "track.exe"
$CompletionFile = Join-Path $CompletionsDir "track.ps1"

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("track-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

try {
    $ArchivePath = Join-Path $TempDir $ArchiveName
    $ChecksumsPath = Join-Path $TempDir "checksums-sha256.txt"
    $ArchiveUrl = "$GitHubReleaseUrl/$Tag/$ArchiveName"
    $ChecksumsUrl = "$GitHubReleaseUrl/$Tag/checksums-sha256.txt"

    Write-Step "Installing track $Version for $Target"
    Write-Step "Download: $ArchiveUrl"

    Download-File -Url $ArchiveUrl -Destination $ArchivePath
    Download-File -Url $ChecksumsUrl -Destination $ChecksumsPath

    Verify-Checksum -ChecksumsPath $ChecksumsPath -ArchivePath $ArchivePath -ArchiveName $ArchiveName
    Write-Step "Checksum verified."

    $ExtractDir = Join-Path $TempDir "extract"
    New-Item -ItemType Directory -Path $ExtractDir -Force | Out-Null
    Expand-Archive -Path $ArchivePath -DestinationPath $ExtractDir -Force

    $ExtractedBinary = Join-Path (Join-Path $ExtractDir $PackageName) "track.exe"
    if (-not (Test-Path $ExtractedBinary)) {
        $Candidate = Get-ChildItem -Path $ExtractDir -Filter "track.exe" -Recurse | Select-Object -First 1
        if (-not $Candidate) {
            Fail "release archive did not contain track.exe"
        }
        $ExtractedBinary = $Candidate.FullName
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path $ExtractedBinary -Destination $TrackExe -Force

    New-Item -ItemType Directory -Path $CompletionsDir -Force | Out-Null
    & $TrackExe completions powershell | Set-Content -Path $CompletionFile -Encoding UTF8

    if ($env:TRACK_SKIP_PATH -eq "1") {
        Write-Step "Skipping user PATH changes because TRACK_SKIP_PATH=1."
    } else {
        Add-InstallDirToUserPath -Directory $InstallDir
    }

    Write-Step ""
    Write-Step "Installation complete."
    Write-Step "  Binary:      $TrackExe"
    Write-Step "  Completion:  $CompletionFile"
    Write-Step ""
    Write-Step "PowerShell completion is installed but not added to your profile automatically."
    Write-Step "Add this line to your PowerShell profile (`$PROFILE) to load it:"
    Write-Step "  . '$CompletionFile'"
    Write-Step ""
    Write-Step "Verify installation:"
    Write-Step "  track --version"
    Write-Step ""
    Write-Step "Optional agent skills:"
    Write-Step "  track init --skills"
} finally {
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}
