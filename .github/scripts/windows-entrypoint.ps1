# Name the symbol behind STATUS_ENTRYPOINT_NOT_FOUND, by asking the loader instead of reading tables.
#
# Reading `dumpbin /exports` does not work for this: a forwarded export carries no RVA, so a table
# comparison calls `kernel32!EnterCriticalSection` and `ole32!CoInitializeEx` missing when they are
# merely forwarded, and an API set contract like `api-ms-win-crt-runtime-l1-1-0.dll` is not a file at
# all. The first version of this step drowned the real answer in forty such false positives.
#
# `LoadLibrary` + `GetProcAddress` is the loader's own resolution: it follows forwarders, resolves
# API sets, and answers about the DLL the process would really get. What it says is what the loader
# will do.

$ErrorActionPreference = "Stop"

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -property installationPath
$tools = Get-ChildItem "$vs\VC\Tools\MSVC\*\bin\Hostx64\x64" | Select-Object -Last 1
$dumpbin = Join-Path $tools.FullName "dumpbin.exe"

Write-Host "OS: $((Get-CimInstance Win32_OperatingSystem).Caption) build $([System.Environment]::OSVersion.Version)"

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class Ldr {
  [DllImport("kernel32", SetLastError=true, CharSet=CharSet.Unicode)]
  public static extern IntPtr LoadLibraryExW(string path, IntPtr reserved, uint flags);
  [DllImport("kernel32", SetLastError=true)]
  public static extern IntPtr GetProcAddress(IntPtr module, string name);
  public const uint ALTERED_SEARCH_PATH = 0x8;
}
"@

# Imports as dumpbin lists them, per DLL. The section dump at the end of the file also has lines of
# the same shape, so the walk stops at "Summary"; ordinal-only imports have no name to ask about.
function Get-Imports($binary) {
  $wanted = [ordered]@{}
  $current = $null
  foreach ($line in (& $dumpbin /imports $binary)) {
    if ($line -match '^\s*Summary\s*$') { break }
    if ($line -match '^\s{4}(\S+\.(?:dll|DLL))\s*$') { $current = $matches[1]; $wanted[$current] = @(); continue }
    if ($current -and $line -match '^\s+[0-9A-F]+\s+([A-Za-z_][A-Za-z0-9_@\?\$]*)\s*$') { $wanted[$current] += $matches[1] }
  }
  return $wanted
}

function Test-Binary($binary) {
  Write-Host "`n=== $(Split-Path $binary -Leaf) ==="
  $named = $false
  foreach ($entry in (Get-Imports $binary).GetEnumerator()) {
    $dll = $entry.Key
    $names = $entry.Value
    if ($names.Count -eq 0) { continue }
    # Prefer the copy beside the binary, which is where the loader looks first and where the pinned
    # libmpv lives; fall back to the plain name so the system search resolves API sets and the rest.
    $beside = Join-Path (Split-Path $binary -Parent) $dll
    $target = if (Test-Path $beside) { $beside } else { $dll }
    $module = [Ldr]::LoadLibraryExW($target, [IntPtr]::Zero, [Ldr]::ALTERED_SEARCH_PATH)
    if ($module -eq [IntPtr]::Zero) {
      Write-Host "  $dll : the loader will not load it (error $([Runtime.InteropServices.Marshal]::GetLastWin32Error()))"
      $named = $true
      continue
    }
    $missing = @($names | Where-Object { [Ldr]::GetProcAddress($module, $_) -eq [IntPtr]::Zero })
    if ($missing.Count -gt 0) {
      Write-Host "  $dll : MISSING $($missing -join ', ')"
      $named = $true
    } else {
      Write-Host "  $dll : $($names.Count) imports, all resolved"
    }
  }
  return $named
}

# Counted, not assumed. This step runs only when the tests already failed, and a build that failed
# to link leaves neither binary on disk, which is exactly when the sentence below used to be printed
# after inspecting nothing.
$named = $false
$inspected = @()
$exe = Get-ChildItem target\debug\deps\video_playback-*.exe -ErrorAction SilentlyContinue | Select-Object -First 1
if ($exe) { $inspected += $exe.FullName; if (Test-Binary $exe.FullName) { $named = $true } }
$mpv = "target\debug\deps\libmpv-2.dll"
if (Test-Path $mpv) { $inspected += $mpv; if (Test-Binary (Resolve-Path $mpv).Path) { $named = $true } }

if ($inspected.Count -eq 0) {
  Write-Host "`nNothing was inspected: neither target\debug\deps\video_playback-*.exe nor"
  Write-Host "target\debug\deps\libmpv-2.dll is on disk. The run failed before it linked them, so"
  Write-Host "this says nothing about any entry point."
} elseif (-not $named) {
  $what = ($inspected | ForEach-Object { Split-Path $_ -Leaf }) -join " and "
  Write-Host "`nEvery named import of $what resolves through the loader. The missing entry point is"
  Write-Host "reached some other way: an ordinal import, or a DLL further down the graph."
}
