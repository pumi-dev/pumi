//! Windows: the zip unpacked with the system's `tar.exe`, and the swap a hidden PowerShell
//! script. A running `.exe` cannot be replaced and the server cannot hand its terminals over
//! (`pumi_sys::caps()`), so the script stops the old server before the renames: the
//! terminals start again from the saved session.
use std::ffi::OsString;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `tar.exe` (bsdtar) reads zips: in System32 since Windows 10 1803, so no zip crate.
pub fn unpack(archive: &Path, into: &Path) -> Command {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let mut tar = Command::new(root.join(r"System32\tar.exe"));
    tar.arg("-xf").arg(archive).arg("-C").arg(into).creation_flags(CREATE_NO_WINDOW);
    tar
}

/// The archive is trusted by the signed manifest alone: the release is not Authenticode-signed.
pub fn check(_app: &Path, _new: &Path) -> Result<(), String> {
    Ok(())
}

/// The swap script, read from `PUMI_SWAP` (which it drops, so the app it opens does not
/// inherit it); its other inputs are `super::swap`'s variables. Console programs it runs are
/// started hidden: with `DETACHED_PROCESS` this PowerShell has no console for them to share.
const SWAP: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
Remove-Item Env:PUMI_SWAP
$app = $env:PUMI_APP; $new = $env:PUMI_NEW; $old = $env:PUMI_OLD; $marker = $env:PUMI_MARKER
$relaunch = $env:PUMI_RELAUNCH -eq '1'
$inside = $app.TrimEnd('\') + '\'
function Start-Pumi { Start-Process -FilePath $env:PUMI_EXE -WorkingDirectory $HOME }
# Every process running a program from the installed app: its server, sidecar, CLI.
function Get-Installed {
  Get-Process | Where-Object { $_.Path -and $_.Path.StartsWith($inside, [StringComparison]::OrdinalIgnoreCase) }
}
# The installed app's server is asked to stop (it saves the session), then whatever still runs
# from the app is ended: Windows renames no directory a running program came from.
function Stop-Installed {
  if (Get-Installed | Where-Object { $_.ProcessName -eq 'pumid' }) {
    $stop = Start-Process -FilePath (Join-Path $app 'bin\pumid.exe') -ArgumentList '--stop' -WindowStyle Hidden -PassThru
    if ($stop) { [void]$stop.WaitForExit(15000) }
  }
  $deadline = (Get-Date).AddSeconds(15)
  while ((Get-Installed) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 200 }
  Get-Installed | Stop-Process -Force
  Start-Sleep -Milliseconds 500
}
# A directory rename, retried: a scanner or the indexer may hold a file in it for a moment.
function Rename-Dir($from, $to) {
  for ($i = 0; $i -lt 20; $i++) {
    try { [IO.Directory]::Move($from, $to); return $true } catch { Start-Sleep -Milliseconds 250 }
  }
  return $false
}
function Remove-Dir($dir) {
  if (Test-Path -LiteralPath $dir) { Remove-Item -LiteralPath $dir -Recurse -Force }
}

$pumi = Get-Process -Id ([int]$env:PUMI_PID)
if ($pumi -and -not $pumi.WaitForExit(120000)) { exit 1 }
Stop-Installed
Remove-Dir $old
if (-not (Rename-Dir $app $old)) {
  if ($relaunch) { Start-Pumi }
  exit 1
}
if (-not (Rename-Dir $new $app)) {
  [void](Rename-Dir $old $app)
  if ($relaunch) { Start-Pumi }
  exit 1
}
Remove-Item -LiteralPath $marker -Force
if (-not $relaunch) { exit 0 }
Start-Pumi
for ($i = 0; $i -lt [int]$env:PUMI_ROLLBACK_AFTER; $i++) {
  if (Test-Path -LiteralPath $marker) { Remove-Dir $old; exit 0 }
  Start-Sleep -Seconds 1
}
# The new version never said it works: put the old one back. Its server goes too, being the
# new version's binary, which holds the directory.
Stop-Installed
Remove-Dir "$app.failed"
if ((Rename-Dir $app "$app.failed") -and (Rename-Dir $old $app)) { Start-Pumi }
"#;

/// Starts the swap script with `env`: hidden, detached from this process's console, and in the
/// home directory, so its working directory holds nothing it renames.
pub fn swap(env: &[(&str, OsString)]) -> std::io::Result<()> {
    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden"])
        .args(["-Command", "& ([scriptblock]::Create($env:PUMI_SWAP))"])
        .envs(env.iter().map(|(key, value)| (key, value)))
        .env("PUMI_SWAP", SWAP)
        .current_dir(pumi_sys::paths::home())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map(drop)
}
