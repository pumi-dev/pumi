# pumi - the installer, for Windows 10 and 11 on x86_64.
#
#   irm https://pumi.dev/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://pumi.dev/install.ps1))) -Version 0.42.0
#   & ([scriptblock]::Create((irm https://pumi.dev/install.ps1))) -Uninstall
#
# Keep this file ASCII: Windows PowerShell 5.1 reads a saved .ps1 without a BOM as cp1252, where
# the bytes of an em dash end a string early. Under `iex` it runs in the caller's session, so it
# never calls `exit` (that would close the window and its error with it): `Fail` throws.
#
# Or straight from the repository:
#
#   irm https://raw.githubusercontent.com/pumi-dev/pumi/main/scripts/release/install.ps1 | iex
#
# What is checked: the release manifest's minisign signature (when `minisign` is installed;
# otherwise only HTTPS vouches for it), and the archive's SHA-256 against that manifest. Those
# two are the whole of the trust here: the executables are unsigned, and SmartScreen never
# looks at a file PowerShell downloaded (it has no mark-of-the-web), so nothing prompts.
#
# Env:
#   PUMI_VERSION=0.42.0        install this version instead of the newest
#   PUMI_DIR=<dir>            where the app goes (%LOCALAPPDATA%\Programs\pumi)
#   PUMI_GITHUB_TOKEN=<tok>   for a release that is not public
#   PUMI_NO_OPEN=1            do not launch pumi when done
#   PUMI_REPO, PUMI_SITE      where releases and the manifest live

param(
  [string]$Version = $env:PUMI_VERSION,
  [string]$Dir = $env:PUMI_DIR,
  [switch]$Uninstall,
  [switch]$NoOpen,
  [switch]$Help
)

# Both are put back when the script ends: under `iex` they are the caller's session's.
$PreviousErrorAction = $ErrorActionPreference
$PreviousProgress = $ProgressPreference
$ErrorActionPreference = "Stop"
# Windows PowerShell 5.1's progress bar slows a large Invoke-WebRequest to a crawl.
$ProgressPreference = "SilentlyContinue"

$App = "pumi"
$Repo = if ($env:PUMI_REPO) { $env:PUMI_REPO } else { "pumi-dev/pumi" }
$Site = if ($env:PUMI_SITE) { $env:PUMI_SITE } else { "https://pumi.dev" }
$Channel = "preview"
$AppId = "dev.pumi.app"
# The update keys (src/updater/manifest.rs): the primary CI signs with, and the backup.
$Keys = @("RWQGWJa1jK6fXLhk2t8/zrWDojiRPRykE5DkbUI0/xcdWcU9YfczkJ79",
  "RWQe/SSsIy1UiLqkZRlz4FJoUwBTAkRbMcEgsddiFiXHblnemzQvvQ2K")

function Say($Message) { Write-Host "==> $Message" }
function Warn($Message) { Write-Host "==> $Message" -ForegroundColor Yellow }
$Tmp = $null
function Restore-Session {
  if ($Tmp) { Remove-Item -LiteralPath $Tmp -Recurse -Force -ErrorAction SilentlyContinue }
  $script:ErrorActionPreference = $PreviousErrorAction
  $script:ProgressPreference = $PreviousProgress
}
function Fail($Message) {
  Write-Host "==> $Message" -ForegroundColor Red
  Restore-Session
  throw "pumi install: $Message"
}

function Usage {
  Write-Host @"
pumi installer

  irm https://pumi.dev/install.ps1 | iex

Options
  -Version <x.y.z>   install a specific version
  -Dir <path>        where pumi goes (%LOCALAPPDATA%\Programs\pumi)
  -NoOpen            do not launch pumi when it is installed
  -Uninstall         remove pumi (settings in %APPDATA%\pumi are left alone)
  -Help

Env: PUMI_VERSION, PUMI_DIR, PUMI_GITHUB_TOKEN, PUMI_NO_OPEN
"@
}

# ---- arguments -------------------------------------------------------------

if ($Help) { Usage; Restore-Session; return }
if ([Environment]::OSVersion.Platform -ne "Win32NT" -or -not $env:LOCALAPPDATA) {
  Fail "this is the Windows installer: on macOS or Linux run https://pumi.dev/install.sh"
}
$Version = $Version -replace '^v', ''
$Open = -not ($NoOpen -or $env:PUMI_NO_OPEN)

# ---- what this machine is --------------------------------------------------

$Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($Arch -ne "X64") { Fail "pumi is built for x86_64 Windows; this machine is $Arch." }
# ConPTY, which every terminal runs in, arrived in Windows 10 1809 (build 17763).
$Build = [Environment]::OSVersion.Version.Build
if ($Build -lt 17763) { Fail "pumi needs Windows 10 1809 or later; this is build $Build." }
$Platform = "windows-x86_64"

$Dest = if ($Dir) { $Dir } else { Join-Path $env:LOCALAPPDATA "Programs\pumi" }
$AppPath = Join-Path $Dest "pumi.app"
$Exe = Join-Path $AppPath "bin\pumi.exe"
$BinDir = Join-Path $AppPath "Resources\bin"
$Cli = Join-Path $BinDir "pumi-cli.exe"
$Shortcut = Join-Path ([Environment]::GetFolderPath("Programs")) "pumi.lnk"
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\pumi"
$Settings = Join-Path $env:APPDATA "pumi"

# ---- what the machine is told about the install ----------------------------

# The user's PATH, not the machine's: no elevation, and nothing changes for another account.
function Add-UserPath($Path) {
  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if (-not $userPath) { [Environment]::SetEnvironmentVariable("Path", $Path, "User"); return $true }
  if (($userPath -split ";") -contains $Path) { return $false }
  $joined = (@($userPath.TrimEnd(";"), $Path) | Where-Object { $_ }) -join ";"
  [Environment]::SetEnvironmentVariable("Path", $joined, "User")
  return $true
}

function Remove-UserPath($Path) {
  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if (-not $userPath) { return }
  $kept = @($userPath -split ";" | Where-Object { $_ -and $_ -ne $Path })
  [Environment]::SetEnvironmentVariable("Path", ($kept -join ";"), "User")
}

# A new PATH is only seen by shells started afterwards; Explorer hands it to them once told
# (WM_SETTINGCHANGE with "Environment").
function Broadcast-Environment {
  if (-not ("Pumi.Broadcast" -as [type])) {
    Add-Type -Namespace Pumi -Name Broadcast -MemberDefinition @"
[System.Runtime.InteropServices.DllImport("user32.dll", SetLastError = true, CharSet = System.Runtime.InteropServices.CharSet.Auto)]
public static extern System.IntPtr SendMessageTimeout(System.IntPtr hwnd, uint msg, System.UIntPtr wparam,
  string lparam, uint flags, uint timeout, out System.UIntPtr result);
"@
  }
  $result = [UIntPtr]::Zero
  [Pumi.Broadcast]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, "Environment", 2, 5000, [ref]$result) | Out-Null
}

# The AppUserModelID on the shortcut is what makes a toast notification say pumi: the app's
# process and the shortcut must carry the same id for Windows to attribute one to the other.
function Set-AppUserModelId($Path, $Id) {
  if (-not ("PumiShortcut" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

[StructLayout(LayoutKind.Sequential)]
public struct PropertyKey { public Guid fmtid; public uint pid; }

[StructLayout(LayoutKind.Explicit)]
public struct PropVariant {
  [FieldOffset(0)] public ushort vt;
  [FieldOffset(8)] public IntPtr pointer;
}

[ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IPropertyStore {
  void GetCount(out uint count);
  void GetAt(uint index, out PropertyKey key);
  void GetValue(ref PropertyKey key, out PropVariant value);
  void SetValue(ref PropertyKey key, ref PropVariant value);
  void Commit();
}

public static class PumiShortcut {
  [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
  static extern void SHGetPropertyStoreFromParsingName(string path, IntPtr parent, uint flags,
    ref Guid iid, out IPropertyStore store);

  public static void SetAppUserModelId(string path, string id) {
    Guid iid = typeof(IPropertyStore).GUID;
    IPropertyStore store;
    // 2 = GPS_READWRITE
    SHGetPropertyStoreFromParsingName(path, IntPtr.Zero, 2, ref iid, out store);
    PropertyKey key = new PropertyKey();
    key.fmtid = new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3");
    key.pid = 5; // PKEY_AppUserModel_ID
    PropVariant value = new PropVariant();
    value.vt = 31; // VT_LPWSTR
    value.pointer = Marshal.StringToCoTaskMemUni(id);
    store.SetValue(ref key, ref value);
    store.Commit();
    Marshal.FreeCoTaskMem(value.pointer);
    Marshal.ReleaseComObject(store);
  }
}
"@
  }
  [PumiShortcut]::SetAppUserModelId($Path, $Id)
}

# `Move-Item` of a directory only works within one volume, and %TEMP% may be another's.
function Move-Dir($From, $To) {
  try { Move-Item -LiteralPath $From -Destination $To -Force -ErrorAction Stop }
  catch {
    Copy-Item -LiteralPath $From -Destination $To -Recurse -Force
    Remove-Item -LiteralPath $From -Recurse -Force
  }
}

# Every process run from the copy this script replaces: the app, and its server (`pumid.exe`)
# and host too. Windows cannot rename a directory a running program came from, so the server
# goes as well, asked first (`--stop` saves the session), as the updater's swap does
# (src/updater/install/windows.rs). Terminals start again from the saved session.
function Get-Installed {
  $inside = $AppPath.TrimEnd('\') + '\'
  @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
      try { $_.Path -and $_.Path.StartsWith($inside, [StringComparison]::OrdinalIgnoreCase) } catch { $false }
    })
}
function Stop-App {
  if ((Get-Installed).Count -eq 0) { return }
  Say "quitting the running copy (terminals restart from the saved session)"
  $pumid = Join-Path $AppPath "bin\pumid.exe"
  if ((Get-Installed | Where-Object { $_.ProcessName -eq 'pumid' }) -and (Test-Path -LiteralPath $pumid)) {
    $stop = Start-Process -FilePath $pumid -ArgumentList '--stop' -WindowStyle Hidden -PassThru
    if ($stop) { [void]$stop.WaitForExit(15000) }
  }
  $deadline = (Get-Date).AddSeconds(15)
  while ((Get-Installed).Count -gt 0 -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 200 }
  Get-Installed | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 500
}

# ---- uninstall -------------------------------------------------------------

if ($Uninstall) {
  Say "removing $AppPath"
  Stop-App
  try { if (Test-Path -LiteralPath $AppPath) { Remove-Item -LiteralPath $AppPath -Recurse -Force } }
  catch { Fail "could not remove $AppPath (is pumi still running?): $($_.Exception.Message)" }
  Remove-Item -LiteralPath (Join-Path $Dest ".pumi.app.old") -Recurse -Force -ErrorAction SilentlyContinue
  Remove-UserPath $BinDir
  Remove-Item -LiteralPath $Shortcut -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $UninstallKey -Recurse -Force -ErrorAction SilentlyContinue
  Write-Host "    settings and plugin data are still in $Settings"
  Restore-Session
  return
}

# ---- where the release is --------------------------------------------------

$Tmp = Join-Path $env:TEMP ("pumi-" + [guid]::NewGuid().ToString("n"))
New-Item -ItemType Directory -Path $Tmp | Out-Null
$Manifest = Join-Path $Tmp "latest.json"
$Signature = Join-Path $Tmp "latest.json.minisig"

# Windows PowerShell 5.1 negotiates TLS 1.0 by default; pumi.dev and GitHub want 1.2 or later.
# (PowerShell 7 builds its own TLS and has no such switch to set.)
if ($PSVersionTable.PSVersion.Major -lt 6) {
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
}

$Token = $env:PUMI_GITHUB_TOKEN
# 5.1 turns a native command's stderr into an error under "Stop": a logged-out gh says so there.
if (-not $Token -and (Get-Command gh -ErrorAction SilentlyContinue)) {
  $previous = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try { $Token = ((& gh auth token 2>$null) | Out-String).Trim() } catch { $Token = $null }
  $ErrorActionPreference = $previous
  if ($LASTEXITCODE -ne 0) { $Token = $null }
}

function Fetch($Url, $OutFile, $Accept) {
  $headers = @{ "User-Agent" = "pumi-installer" }
  if ($Accept) { $headers["Accept"] = $Accept }
  if ($Token -and $Url.StartsWith("https://api.github.com/")) { $headers["Authorization"] = "Bearer $Token" }
  Invoke-WebRequest -Uri $Url -Headers $headers -OutFile $OutFile -UseBasicParsing
}

$Mode = $null
# The newest release's manifest, published on the site.
if (-not $Version) {
  try {
    Fetch "$Site/releases/$Channel/latest.json" $Manifest
    Fetch "$Site/releases/$Channel/latest.json.minisig" $Signature
    if (-not (Get-Content -Raw $Manifest).TrimStart().StartsWith("{")) { throw "not a manifest" }
    $Mode = "site"
  } catch { $Mode = $null }
}
if (-not $Mode) {
  $Api = if ($Version) { "https://api.github.com/repos/$Repo/releases/tags/v$Version" }
  else { "https://api.github.com/repos/$Repo/releases/latest" }
  try { Fetch $Api (Join-Path $Tmp "release.json") "application/vnd.github+json" }
  catch {
    if (-not $Token) {
      Fail "could not find a pumi release$(if ($Version) { " $Version" }). If the repository is private, set
   PUMI_GITHUB_TOKEN (a token that can read $Repo), or log in with ``gh auth login``."
    }
    Fail "could not find a pumi release$(if ($Version) { " $Version" }) in $Repo."
  }
  $Mode = "github"
}

# The release's assets, by name: each asset's API URL, from the release JSON.
$AssetUrls = @{}
if ($Mode -eq "github") {
  $Release = Get-Content -Raw (Join-Path $Tmp "release.json") | ConvertFrom-Json
  foreach ($asset in $Release.assets) { $AssetUrls[$asset.name] = $asset.url }
}

# Get-Asset <name> <out> [url]: a release file, from its public URL or through the API.
function Get-Asset($Name, $OutFile, $Url) {
  if ($Mode -eq "site" -and $Url) { Fetch $Url $OutFile; return }
  if (-not $AssetUrls.ContainsKey($Name)) { return }
  Fetch $AssetUrls[$Name] $OutFile "application/octet-stream"
}

if ($Mode -eq "github") {
  Get-Asset "latest.json" $Manifest
  Get-Asset "latest.json.minisig" $Signature
  if (-not (Test-Path $Manifest)) { Fail "the release has no latest.json" }
  if (-not (Test-Path $Signature)) { Fail "the release has no latest.json.minisig" }
}

# ---- prove what we got -----------------------------------------------------

$Minisign = Get-Command minisign -ErrorAction SilentlyContinue
if ($Minisign) {
  $verified = $null
  foreach ($key in $Keys) {
    # A native command's output is stderr's as much as stdout's; that is not this script failing.
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $out = (& $Minisign.Source -V -m $Manifest -x $Signature -P $key 2>&1) | Out-String
    $ErrorActionPreference = $previous
    if ($LASTEXITCODE -eq 0 -and $out -match 'file:latest\.json') { $verified = $key; break }
  }
  if (-not $verified) { Fail "the release manifest is not signed by a pumi release key." }
  Say "signature:   the manifest is signed by pumi's release key"
} else {
  Warn "signature:   not checked (install minisign to check it); trusting HTTPS"
}

$Info = Get-Content -Raw $Manifest | ConvertFrom-Json
$ManifestVersion = $Info.version
$Asset = $Info.assets.$Platform
if (-not $Asset -or -not $Asset.name -or -not $Asset.sha256) {
  Fail "pumi $ManifestVersion has no build for $Platform."
}

# ---- fetch -----------------------------------------------------------------

Say "downloading pumi $ManifestVersion for $Platform"
$Archive = Join-Path $Tmp $Asset.name
Get-Asset $Asset.name $Archive $Asset.url
if (-not (Test-Path $Archive)) { Fail "download failed: $($Asset.name)" }
$have = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($have -ne $Asset.sha256.ToLowerInvariant()) {
  Fail "the download does not match the signed manifest.
   expected $($Asset.sha256)
   got      $have"
}
Say "checksum:    matches the manifest"

# ---- install ---------------------------------------------------------------

$Unpacked = Join-Path $Tmp "unpacked"
# Windows' own bsdtar (there since 1803): Expand-Archive is many times slower on 5.1.
New-Item -ItemType Directory -Path $Unpacked -Force | Out-Null
& (Join-Path $env:SystemRoot "System32\tar.exe") -xf $Archive -C $Unpacked
if ($LASTEXITCODE -ne 0) { Fail "could not unpack $($Asset.name)" }
$New = Join-Path $Unpacked "pumi.app"
if (-not (Test-Path (Join-Path $New "bin\pumi.exe"))) { Fail "the download holds no pumi.app" }

Stop-App

Say "installing to $AppPath"
New-Item -ItemType Directory -Path $Dest -Force | Out-Null
$Old = Join-Path $Dest ".pumi.app.old"
Remove-Item -LiteralPath $Old -Recurse -Force -ErrorAction SilentlyContinue
# Two renames, as on macOS: a failure half way puts the old copy back.
if (Test-Path -LiteralPath $AppPath) {
  try { Move-Item -LiteralPath $AppPath -Destination $Old -Force -ErrorAction Stop }
  catch { Fail "could not replace $AppPath : $($_.Exception.Message)" }
}
try { Move-Dir $New $AppPath }
catch {
  if (Test-Path -LiteralPath $Old) { Move-Dir $Old $AppPath }
  Fail "could not write $AppPath : $($_.Exception.Message)"
}
Remove-Item -LiteralPath $Old -Recurse -Force -ErrorAction SilentlyContinue

if (Add-UserPath $BinDir) {
  Broadcast-Environment
  $env:Path = "$env:Path;$BinDir"
  Say "added $BinDir to your PATH (new shells only)"
}

# The Start Menu entry: where a user (and the uninstall list) finds pumi, and the id that makes
# its toast notifications its own.
$Shell = New-Object -ComObject WScript.Shell
$Link = $Shell.CreateShortcut($Shortcut)
$Link.TargetPath = $Exe
# Home, not the app's directory: a process whose working directory is inside pumi.app holds it,
# and an update cannot rename it then.
$Link.WorkingDirectory = $HOME
$Link.IconLocation = "$Exe,0"
$Link.Description = "pumi"
$Link.Save()
Set-AppUserModelId $Shortcut $AppId

# The updater reads the token from here while the repository is private (src/updater/fetch.rs).
if ($env:PUMI_GITHUB_TOKEN) {
  New-Item -ItemType Directory -Path $Settings -Force | Out-Null
  [IO.File]::WriteAllText((Join-Path $Settings "github-token"), $env:PUMI_GITHUB_TOKEN.Trim())
}

# Settings -> Apps lists pumi through this, and its Uninstall runs this same script, for this
# directory.
$DirArg = if ($Dir) { " -Dir '" + ($Dest -replace "'", "''") + "'" } else { "" }
$Uninstaller = "powershell -NoProfile -ExecutionPolicy Bypass -Command `"& ([scriptblock]::Create((Invoke-RestMethod $Site/install.ps1))) -Uninstall$DirArg`""
New-Item -Path $UninstallKey -Force | Out-Null
Set-ItemProperty -Path $UninstallKey -Name DisplayName -Value "pumi"
Set-ItemProperty -Path $UninstallKey -Name DisplayVersion -Value $ManifestVersion
Set-ItemProperty -Path $UninstallKey -Name Publisher -Value "pumi"
Set-ItemProperty -Path $UninstallKey -Name DisplayIcon -Value $Exe
Set-ItemProperty -Path $UninstallKey -Name InstallLocation -Value $AppPath
Set-ItemProperty -Path $UninstallKey -Name UninstallString -Value $Uninstaller
New-ItemProperty -Path $UninstallKey -Name NoModify -PropertyType DWord -Value 1 -Force | Out-Null
New-ItemProperty -Path $UninstallKey -Name NoRepair -PropertyType DWord -Value 1 -Force | Out-Null

Say "pumi $ManifestVersion is installed"
Write-Host "    app:      $AppPath"
Write-Host "    cli:      $Cli (as ``pumi`` on a new shell's PATH)"
Write-Host "    settings: $Settings\settings.json"
Write-Host "    updates:  automatic - pumi tells you when one is ready (Settings -> General -> Updates)"
Write-Host "    docs:     $Site/docs/"

Restore-Session
if ($Open) { Start-Process -FilePath $Exe -WorkingDirectory $HOME }
