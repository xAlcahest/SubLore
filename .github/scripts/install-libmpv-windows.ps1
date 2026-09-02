# The libmpv the Windows build links against: a pinned dev archive, its checksum, and the MSVC
# import library the archive does not ship. Two jobs need it, so it lives here rather than being
# copied into the second one. See BACKLOG.md M0.2 and M0.3.
$ErrorActionPreference = "Stop"
$tag  = "20260814"
$file = "mpv-dev-x86_64-20260814-git-7b8915bc1d.7z"
$sha  = "0af22b28e920620036d3ae08fd9283156dc9af0420bf4df84b0e02282094599c"
$dir  = "$env:RUNNER_TEMP\mpv-dev"
Invoke-WebRequest -Uri "https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/$tag/$file" -OutFile mpv-dev.7z
if ((Get-FileHash mpv-dev.7z -Algorithm SHA256).Hash -ne $sha.ToUpper()) { throw "libmpv archive checksum mismatch" }
7z x mpv-dev.7z -o"$dir" | Out-Null

# MSVC import library: the archive ships only a MinGW .dll.a, so build mpv.lib from the DLL exports.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs    = & $vswhere -latest -products * -property installationPath
$tools = Get-ChildItem "$vs\VC\Tools\MSVC\*\bin\Hostx64\x64" | Select-Object -Last 1
$dumpbin = Join-Path $tools.FullName "dumpbin.exe"
$libexe  = Join-Path $tools.FullName "lib.exe"
$names = & $dumpbin /exports "$dir\libmpv-2.dll" |
  Select-String -Pattern '^\s+\d+\s+[0-9A-F]+\s+[0-9A-F]{8}\s+(mpv_\S+)' |
  ForEach-Object { $_.Matches[0].Groups[1].Value }
if ($names.Count -lt 50) { throw "expected at least 50 mpv_* exports, found $($names.Count)" }
if ($names -notcontains "mpv_create") { throw "mpv_create missing from libmpv-2.dll exports" }
@("EXPORTS") + $names | Set-Content "$dir\mpv.def"
& $libexe /def:"$dir\mpv.def" /name:libmpv-2.dll /out:"$dir\mpv.lib" /MACHINE:X64
if (-not (Test-Path "$dir\mpv.lib")) { throw "failed to generate mpv.lib" }

"LIBMPV_LIB_DIR=$dir" | Out-File -Append -Encoding utf8 $env:GITHUB_ENV
"$dir" | Out-File -Append -Encoding utf8 $env:GITHUB_PATH
