# cc-code Agent Installer for Windows
# Usage: irm https://raw.githubusercontent.com/cc-claws/cc-code/main/scripts/install.ps1 | iex
#
# Options:
#   $env:CC_CODE_INSTALL_VERSION   Specific version tag (e.g. npm-v0.6.89), empty = latest
#   $env:CC_CODE_INSTALL_DIR       Install directory (default: $env:USERPROFILE\.cc-code)
#                                  Legacy alias: $env:CCCODE_INSTALL_DIR (kept for compatibility)
#   $env:GITHUB_PROXY           GitHub download proxy prefix (replaces https://github.com)
#   $env:GITHUB_TOKEN           GitHub personal access token (bypasses API rate limiting)
#   $env:CC_CODE_NO_PATH_HINT      Set to 1 to skip PATH hint
#   $env:CC_CODE_INSTALL_PLATFORM  Override platform detection (e.g. windows-x86_64)
#
# Example:
#   $env:CC_CODE_INSTALL_VERSION="npm-v0.6.89"; irm ... | iex
#   $env:GITHUB_PROXY="https://ghproxy.com/https://github.com"; irm ... | iex

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

# Enable modern TLS protocols (TLS 1.2 = 3072, TLS 1.3 = 12288)
try {
    [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072 -bor 12288
} catch {}

# Configure DefaultWebProxy from environment if present (HTTP/HTTPS proxies)
$script:EnvProxy = if ($env:HTTPS_PROXY) { $env:HTTPS_PROXY }
                   elseif ($env:https_proxy) { $env:https_proxy }
                   elseif ($env:HTTP_PROXY) { $env:HTTP_PROXY }
                   elseif ($env:http_proxy) { $env:http_proxy }
                   elseif ($env:ALL_PROXY) { $env:ALL_PROXY }
                   elseif ($env:all_proxy) { $env:all_proxy }
                   else { $null }

if ($script:EnvProxy -and ($script:EnvProxy -notmatch '^socks5h?://')) {
    try {
        [System.Net.WebRequest]::DefaultWebProxy = New-Object System.Net.WebProxy($script:EnvProxy)
    } catch {}
}

# --- Logging ---
function info  { Write-Host "[INFO]  $args" -ForegroundColor Green }
function warn  { Write-Host "[WARN]  $args" -ForegroundColor Yellow }
function error { Write-Host "[ERROR] $args" -ForegroundColor Red }
function step  { Write-Host "[STEP]  $args" -ForegroundColor Cyan }

# --- Platform Detection ---
function Detect-Platform {
    if ($env:CC_CODE_INSTALL_PLATFORM) {
        if ($env:CC_CODE_INSTALL_PLATFORM -notmatch '^(macos|linux|windows)-(x86_64|aarch64|riscv64)$') {
            error "Invalid CC_CODE_INSTALL_PLATFORM: $env:CC_CODE_INSTALL_PLATFORM"
            Write-Host "  Expected: macos-x86_64 | macos-aarch64 | linux-x86_64 | linux-aarch64 | linux-riscv64 | windows-x86_64"
            exit 1
        }
        info "Platform (manual): $env:CC_CODE_INSTALL_PLATFORM"
        return $env:CC_CODE_INSTALL_PLATFORM
    }

    $arch = switch ($env:PROCESSOR_ARCHITECTURE) {
        "AMD64" { "x86_64" }
        "ARM64" { "aarch64" }
        default {
            error "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE"
            exit 1
        }
    }

    $platform = "windows-${arch}"
    info "Detected platform: $platform"
    return $platform
}

# --- Download with optional proxy ---
function Get-DownloadUrl {
    param([string]$Url)
    if ($env:GITHUB_PROXY) {
        return $Url -replace 'https://github\.com', $env:GITHUB_PROXY
    }
    return $Url
}

# --- Reliable File Download (curl.exe with Invoke-WebRequest fallback) ---
function Download-File {
    param(
        [string]$Url,
        [string]$OutFile
    )
    # Prefer Windows built-in curl.exe (available on Win10 1803+ and Win11).
    # curl natively honors HTTP(S)_PROXY/ALL_PROXY, handles 302 redirects,
    # TLS 1.3, and supports automatic retries.
    $curl = Get-Command "curl.exe" -ErrorAction SilentlyContinue
    if ($curl) {
        $curlArgs = @("-fL", "--retry", "3", "--connect-timeout", "15", "-o", $OutFile, $Url)
        try {
            & $curl.Source @curlArgs 2>$null
            if ($LASTEXITCODE -eq 0 -and (Test-Path $OutFile)) {
                return
            }
        } catch {}
    }

    # Fallback to Invoke-WebRequest
    Invoke-WebRequest -Uri $Url -OutFile $OutFile
}

# --- GitHub API request (with optional token & curl fallback) ---
function Invoke-GitHubApi {
    param([string]$Url)
    $headers = @{}
    if ($env:GITHUB_TOKEN) {
        $headers["Authorization"] = "Bearer $env:GITHUB_TOKEN"
    }

    try {
        $response = Invoke-RestMethod -Uri $Url -Headers $headers -ErrorAction Stop
        return $response
    } catch {
        $curl = Get-Command "curl.exe" -ErrorAction SilentlyContinue
        if ($curl) {
            $curlArgs = @("-fsSL", "--retry", "2", "--connect-timeout", "10")
            if ($env:GITHUB_TOKEN) {
                $curlArgs += @("-H", "Authorization: Bearer $env:GITHUB_TOKEN")
            }
            $curlArgs += $Url
            try {
                $json = & $curl.Source @curlArgs 2>$null
                if ($LASTEXITCODE -eq 0 -and $json) {
                    return ($json | ConvertFrom-Json)
                }
            } catch {}
        }
        throw $_
    }
}

# --- SHA256 Verification ---
# release 产物里带 checksums.txt（sha256sum 格式）。下载完 zip 必须校验摘要（#588）：
# 与 npm/install.js 的 verifyChecksum 对齐——checksums.txt 下载失败、条目缺失或
# 摘要不匹配，一律拒绝安装（fail-closed），绝不把未校验的二进制落到安装目录。
function Get-ExpectedSha256 {
    param([string]$ChecksumsPath, [string]$FileName)
    foreach ($line in Get-Content -Path $ChecksumsPath) {
        $m = [regex]::Match($line.Trim(), '^([0-9a-fA-F]{64})\s+\*?(.+)$')
        if ($m.Success -and $m.Groups[2].Value.Trim() -eq $FileName) {
            return $m.Groups[1].Value.ToLower()
        }
    }
    return $null
}

# --- 计算文件 SHA256（Get-FileHash 缺失时的回退） ---
# 部分环境（例如 PSModulePath 混入了 PowerShell 7 的模块目录）会让 Windows PowerShell 5.1
# 加载 Microsoft.PowerShell.Utility 失败、Get-FileHash 不可用；校验环节不应成为安装的
# 唯一断点，此时退回系统自带的 certutil（输出同样的 SHA256 十六进制摘要）。
function Get-Sha256Hex {
    param([string]$Path)
    if (Get-Command Get-FileHash -ErrorAction SilentlyContinue) {
        return (Get-FileHash -Path $Path -Algorithm SHA256).Hash.ToLower()
    }
    try {
        $lines = & certutil.exe -hashfile $Path SHA256 2>$null
    } catch {
        return $null
    }
    if ($LASTEXITCODE -ne 0) {
        return $null
    }
    foreach ($line in $lines) {
        $trimmed = $line -replace '\s', ''
        if ($trimmed -match '^[0-9a-fA-F]{64}$') {
            return $trimmed.ToLower()
        }
    }
    return $null
}

# --- Cleanup Old Versions ---
function Clean-OldVersions {
    param([string]$InstallDir, [string]$CurrentVersion)

    # Collect agent-v* directories, excluding current version
    $oldDirs = @(Get-ChildItem -Path $InstallDir -Directory | Where-Object {
        $_.Name -match '^npm-v' -and $_.Name -ne $CurrentVersion
    })

    if ($oldDirs.Count -eq 0) {
        info "No old versions to clean up."
        return
    }

    Write-Host ""
    warn "Found $($oldDirs.Count) old version(s):"
    $totalSize = 0
    foreach ($d in $oldDirs) {
        $size = (Get-ChildItem -Path $d.FullName -Recurse -File -ErrorAction SilentlyContinue |
                 Measure-Object -Property Length -Sum).Sum
        if (-not $size) { $size = 0 }
        $totalSize += $size
        $sizeMB = [math]::Round($size / 1MB, 1)
        Write-Host "  $($d.Name)  ($sizeMB MB)"
    }
    $totalMB = [math]::Round($totalSize / 1MB, 1)
    Write-Host "  Total: $totalMB MB"
    Write-Host ""

    $answer = Read-Host "Delete old versions? [y/N]"
    switch ($answer) {
        { $_ -match '^[yY](es)?$' } {
            foreach ($d in $oldDirs) {
                Remove-Item -Recurse -Force $d.FullName
                info "Removed: $($d.Name)"
            }
            info "Cleaned up $($oldDirs.Count) old version(s)."
        }
        default {
            info "Skipped cleanup."
        }
    }
}

# --- Main ---
function Main {
    # 优先文档名 CC_CODE_INSTALL_DIR；CCCODE_INSTALL_DIR 是历史拼写，保留兼容（#588）
    $InstallDir = if ($env:CC_CODE_INSTALL_DIR) {
        $env:CC_CODE_INSTALL_DIR
    } elseif ($env:CCCODE_INSTALL_DIR) {
        $env:CCCODE_INSTALL_DIR
    } else {
        Join-Path $env:USERPROFILE ".cc-code"
    }
    $GitHubApi = "https://api.github.com/repos/cc-claws/cc-code"

    Write-Host ""
    info "cc-code Agent Installer (Windows)"
    info "-------------------------------"

    $Platform = Detect-Platform
    $AssetName = "cc-code-${Platform}.zip"
    $ExeName = "cc-code.exe"

    # Fetch release info
    if ($env:CC_CODE_INSTALL_VERSION) {
        $VersionTag = $env:CC_CODE_INSTALL_VERSION
        step "Fetching release: $VersionTag..."
        try {
            $Release = Invoke-GitHubApi "$GitHubApi/releases/tags/$VersionTag"
        } catch {
            error "Failed to fetch release '$VersionTag'. Does this tag exist?"
            exit 1
        }
    } else {
        step "Fetching latest agent release..."
        try {
            $Releases = Invoke-GitHubApi "$GitHubApi/releases?per_page=30"
        } catch {
            error "Failed to fetch releases from GitHub."
            exit 1
        }

        # Find latest npm-v* tag
        $VersionTag = ($Releases | Where-Object { $_.tag_name -like "npm-v*" } | Select-Object -First 1).tag_name
        if (-not $VersionTag) {
            error "No agent release found."
            exit 1
        }

        try {
            $Release = Invoke-GitHubApi "$GitHubApi/releases/tags/$VersionTag"
        } catch {
            error "Failed to fetch release '$VersionTag'."
            exit 1
        }
    }

    info "Found release: $VersionTag"

    # Find matching asset
    $Asset = $Release.assets | Where-Object { $_.name -eq $AssetName }
    if (-not $Asset) {
        error "No binary found for platform '$Platform'."
        Write-Host ""
        Write-Host "Available assets:"
        $Release.assets | ForEach-Object { Write-Host "  - $($_.name)" }
        exit 1
    }

    $DownloadUrl = $Asset.browser_download_url
    info "Binary: $AssetName"

    # Create install directory
    $VersionDir = Join-Path $InstallDir $VersionTag
    New-Item -ItemType Directory -Force -Path $VersionDir | Out-Null

    $ZipPath = Join-Path $VersionDir $AssetName

    # Download
    $FinalUrl = Get-DownloadUrl $DownloadUrl
    if ($FinalUrl -ne $DownloadUrl) {
        info "Using proxy: $FinalUrl"
    }

    step "Downloading..."
    try {
        Download-File -Url $FinalUrl -OutFile $ZipPath
    } catch {
        error "Download failed: $_"
        exit 1
    }

    # Verify checksum before extracting a single byte (#588)
    step "Verifying checksum..."
    $ChecksumsAsset = $Release.assets | Where-Object { $_.name -eq "checksums.txt" } | Select-Object -First 1
    if (-not $ChecksumsAsset) {
        Remove-Item -Force $ZipPath -ErrorAction SilentlyContinue
        error "Release $VersionTag does not publish checksums.txt."
        Write-Host "  Refusing to install an unverified binary. Nothing was installed."
        exit 1
    }

    $ChecksumsPath = Join-Path $VersionDir "checksums.txt"
    try {
        Download-File -Url (Get-DownloadUrl $ChecksumsAsset.browser_download_url) -OutFile $ChecksumsPath
    } catch {
        Remove-Item -Force $ZipPath -ErrorAction SilentlyContinue
        Remove-Item -Force $ChecksumsPath -ErrorAction SilentlyContinue
        error "Failed to download checksums.txt. Refusing to install (fail-closed): $_"
        exit 1
    }

    $ExpectedSha = Get-ExpectedSha256 -ChecksumsPath $ChecksumsPath -FileName $AssetName
    Remove-Item -Force $ChecksumsPath -ErrorAction SilentlyContinue
    if (-not $ExpectedSha) {
        Remove-Item -Force $ZipPath -ErrorAction SilentlyContinue
        error "checksums.txt has no entry for $AssetName. Refusing to install (fail-closed)."
        exit 1
    }

    $ActualSha = Get-Sha256Hex -Path $ZipPath
    if ($ActualSha -ne $ExpectedSha) {
        Remove-Item -Force $ZipPath -ErrorAction SilentlyContinue
        # `${AssetName}` 的花括号不可省：直接写 `$AssetName:` 会被 PowerShell 当作
        # 变量名加驱动器限定（解析期即报 "变量引用无效"），整个安装脚本一行都执行不了。
        error "Checksum mismatch for ${AssetName}: the download may be corrupted or tampered with."
        Write-Host "    expected: $ExpectedSha"
        Write-Host "    actual:   $ActualSha"
        error "Refusing to install. Nothing was installed."
        exit 1
    }
    info "Checksum OK: $ActualSha"

    # Extract
    step "Extracting..."
    try {
        Expand-Archive -Path $ZipPath -DestinationPath $VersionDir -Force
    } catch {
        error "Extraction failed: $_"
        exit 1
    }
    Remove-Item -Force $ZipPath -ErrorAction SilentlyContinue

    # Zip contains cc-code-<platform>.exe (e.g. cc-code-windows-x86_64.exe), find and rename to cc-code.exe
    $SourceExe = Get-ChildItem -Path $VersionDir -Recurse -Filter "*.exe" | Where-Object { $_.Name -notlike "unins*" } | Select-Object -First 1
    if (-not $SourceExe) {
        error "No .exe found in extracted archive."
        Get-ChildItem -Path $VersionDir -Recurse | ForEach-Object { Write-Host "  $($_.FullName)" }
        exit 1
    }

    $TargetExe = Join-Path $VersionDir $ExeName
    if ($SourceExe.FullName -ne $TargetExe) {
        Move-Item -Force $SourceExe.FullName $TargetExe
    }

    info "Installed to: $TargetExe"

    # Create convenience copy (Windows doesn't support symlinks without admin).
    # $LinkPath may be the *running* executable, which Windows locks against
    # being overwritten; rename it aside first (a running image may be renamed)
    # and retry, mirroring npm/install.js's replaceBinary strategy.
    $LinkPath = Join-Path $InstallDir $ExeName
    try {
        Copy-Item -Force -ErrorAction Stop $TargetExe $LinkPath
    } catch {
        $stamp = Get-Date -Format "yyyyMMddHHmmss"
        try {
            Move-Item -Force -ErrorAction Stop $LinkPath "$LinkPath.old.$stamp"
            Copy-Item -Force -ErrorAction Stop $TargetExe $LinkPath
            warn "Previous $ExeName was in use; replaced (old copy kept as $ExeName.old.$stamp)."
        } catch {
            warn "$ExeName is in use and could not be replaced now; the new version is at $VersionDir and will be used after the running instance exits."
        }
    }

    # Write current version
    $VersionFile = Join-Path $InstallDir "current-version.txt"
    $VersionTag | Out-File -FilePath $VersionFile -Encoding ascii -NoNewline

    # --- PATH Setup ---
    if ($env:CC_CODE_NO_PATH_HINT -ne "1") {
        $currentPath = [Environment]::GetEnvironmentVariable("Path", "User") -split ";"
        $installPathNormalized = (Resolve-Path $InstallDir).Path.TrimEnd("\")

        # Check if install dir is already in PATH (case-insensitive)
        $alreadyInPath = $false
        foreach ($p in $currentPath) {
            if ($p.TrimEnd("\") -eq $installPathNormalized) {
                $alreadyInPath = $true
                break
            }
        }

        if (-not $alreadyInPath) {
            [Environment]::SetEnvironmentVariable("Path", "$InstallDir;$([Environment]::GetEnvironmentVariable('Path', 'User'))", "User")
            info "Added $InstallDir to user PATH"

            # Refresh current session's PATH
            $env:Path = "$InstallDir;$env:Path"
        }
    }

    # Offer to clean up old versions
    Clean-OldVersions -InstallDir $InstallDir -CurrentVersion $VersionTag

    Write-Host ""
    info "Installation complete! Version: $VersionTag"
    Write-Host ""
    info "Open a new terminal and run 'cc-code' to start."
    Write-Host ""
}

Main
