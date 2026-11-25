param(
    [Parameter(Mandatory=$false)]
    [string]$ExtensionId = "",
    [string]$HostExePath = ""
)

if ([string]::IsNullOrWhiteSpace($HostExePath)) {
    $HostExePath = Join-Path $PSScriptRoot "target\release\turboget-host.exe"
}

$ManifestPath = Join-Path $PSScriptRoot "com.turboget.host.installed.json"
$ResolvedExe = (Resolve-Path -LiteralPath $HostExePath -ErrorAction Stop).Path
$RepoRoot = Split-Path -Parent $PSScriptRoot
$ExtManifestFile = Join-Path $RepoRoot "extension\manifest.json"

$DiscoveredIds = New-Object System.Collections.Generic.List[string]
if (-not [string]::IsNullOrWhiteSpace($ExtensionId)) {
    $DiscoveredIds.Add($ExtensionId.Trim().ToLower())
}

# 1. Derive deterministic ID from extension/manifest.json "key" if present
if (Test-Path -LiteralPath $ExtManifestFile) {
    try {
        $extManifest = Get-Content $ExtManifestFile -Raw | ConvertFrom-Json
        if ($extManifest.key) {
            $der = [Convert]::FromBase64String([string]$extManifest.key)
            $sha = [System.Security.Cryptography.SHA256]::Create()
            $hash = $sha.ComputeHash($der)
            $sha.Dispose()
            $alphabet = "abcdefghijklmnop"
            $derivedId = ""
            for ($i = 0; $i -lt 16; $i++) {
                $b = $hash[$i]
                $derivedId += $alphabet[($b -shr 4) -band 0xF]
                $derivedId += $alphabet[$b -band 0xF]
            }
            if (-not $DiscoveredIds.Contains($derivedId)) {
                $DiscoveredIds.Add($derivedId)
            }
        }
    } catch {}
}

# Fallback known fixed ID
if (-not $DiscoveredIds.Contains("mbikddgphpokomgjlnkgipejppbjileo")) {
    $DiscoveredIds.Add("mbikddgphpokomgjlnkgipejppbjileo")
}

# 2. Auto-discover any loaded unpacked TFM/TurboGet Extension IDs from Chrome/Edge/Brave profiles
$BrowserUserDirs = @(
    "$env:LOCALAPPDATA\Google\Chrome\User Data",
    "$env:LOCALAPPDATA\Microsoft\Edge\User Data",
    "$env:LOCALAPPDATA\BraveSoftware\Brave-Browser\User Data"
)

foreach ($userDir in $BrowserUserDirs) {
    if (-not (Test-Path $userDir)) { continue }
    Get-ChildItem -Path $userDir -Directory -ErrorAction SilentlyContinue | ForEach-Object {
        foreach ($prefName in @("Secure Preferences", "Preferences")) {
            $prefPath = Join-Path $_.FullName $prefName
            if (-not (Test-Path -LiteralPath $prefPath)) { continue }
            try {
                $json = Get-Content $prefPath -Raw -ErrorAction Stop | ConvertFrom-Json
                if ($json.extensions -and $json.extensions.settings) {
                    $json.extensions.settings.PSObject.Properties | ForEach-Object {
                        $id = [string]$_.Name
                        $entry = $_.Value
                        $mName = if ($entry.manifest -and $entry.manifest.name) { [string]$entry.manifest.name } else { "" }
                        $ePath = if ($entry.path) { [string]$entry.path } else { "" }
                        if (
                            $mName -match "Take File Manager|TurboGet" -or
                            $ePath -match "[\\/]idm[\\/]extension$" -or
                            $ePath -like "*Take File Manager*extension*"
                        ) {
                            $cleanId = $id.Trim().ToLower()
                            if ($cleanId.Length -eq 32 -and -not $DiscoveredIds.Contains($cleanId)) {
                                $DiscoveredIds.Add($cleanId)
                            }
                        }
                    }
                }
            } catch {}
        }
    }
}

$AllowedOrigins = @($DiscoveredIds | ForEach-Object { "chrome-extension://$_/" })

$Manifest = @{
    name = "com.turboget.host"
    description = "Take File Manager Native Messaging Host"
    path = $ResolvedExe
    type = "stdio"
    allowed_origins = $AllowedOrigins
} | ConvertTo-Json -Depth 3

Set-Content -Path $ManifestPath -Value $Manifest -Encoding UTF8

$RegPaths = @(
    "HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.turboget.host",
    "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\com.turboget.host",
    "HKCU:\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.turboget.host"
)

foreach ($RegPath in $RegPaths) {
    if (-not (Test-Path $RegPath)) {
        New-Item -Path $RegPath -Force | Out-Null
    }
    Set-ItemProperty -Path $RegPath -Name "(Default)" -Value $ManifestPath
    Write-Host "Registered: $RegPath -> $ManifestPath"
}

Write-Host "Allowed Extension IDs: $($DiscoveredIds -join ', ')"

# Firefox Native Messaging manifest (uses allowed_extensions instead of allowed_origins)
$FirefoxManifestPath = Join-Path $PSScriptRoot "com.turboget.host.firefox.installed.json"
$FirefoxManifest = @{
    name = "com.turboget.host"
    description = "Take File Manager Native Messaging Host"
    path = $ResolvedExe
    type = "stdio"
    allowed_extensions = @("takefilemanager@turboget.app")
} | ConvertTo-Json -Depth 3
Set-Content -Path $FirefoxManifestPath -Value $FirefoxManifest -Encoding UTF8

$FirefoxRegPath = "HKCU:\Software\Mozilla\NativeMessagingHosts\com.turboget.host"
if (-not (Test-Path $FirefoxRegPath)) {
    New-Item -Path $FirefoxRegPath -Force | Out-Null
}
Set-ItemProperty -Path $FirefoxRegPath -Name "(Default)" -Value $FirefoxManifestPath
Write-Host "Registered Firefox: $FirefoxRegPath -> $FirefoxManifestPath"

$TurboGetExe = Join-Path $RepoRoot "src-tauri\target\release\turboget.exe"
if (Test-Path -LiteralPath $TurboGetExe) {
    $MagnetBase = "HKCU:\Software\Classes\magnet"
    $MagnetCmd = "$MagnetBase\shell\open\command"
    New-Item -Path $MagnetCmd -Force | Out-Null
    Set-ItemProperty -Path $MagnetBase -Name "(Default)" -Value "URL:Magnet Link"
    Set-ItemProperty -Path $MagnetBase -Name "URL Protocol" -Value ""
    Set-ItemProperty -Path $MagnetCmd -Name "(Default)" -Value "`"$TurboGetExe`" `"%1`""
    Write-Host "Registered magnet protocol -> $TurboGetExe"
}
