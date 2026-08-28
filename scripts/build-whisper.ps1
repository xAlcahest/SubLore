# Build the whisper.cpp sidecar binaries on Windows. The PowerShell half of
# scripts/build-whisper.sh; both read whisper.pin and produce the same two files in .whisper\bin.
# See BACKLOG.md M3.1.
#
# Unverified: there is no Windows machine in this project yet, so this script is written from the
# POSIX one and has never been run. Say so if it fails rather than assuming the build is broken.
#
# Prerequisites: git, cmake, a C++ toolchain (Visual Studio Build Tools), and for the Vulkan build
# the LunarG Vulkan SDK, which provides glslc, the headers and SPIRV-Headers.

[CmdletBinding()]
param(
    [switch]$CpuOnly,
    [int]$Jobs = 0
)

$ErrorActionPreference = "Stop"

function Fail($message) {
    Write-Error "build-whisper: $message"
    exit 1
}

$repoRoot = (& git rev-parse --show-toplevel)
if ($LASTEXITCODE -ne 0) { Fail "not inside a git work tree" }
Set-Location $repoRoot

if (-not (Test-Path "whisper.pin")) { Fail "whisper.pin is missing" }
$pin = Get-Content "whisper.pin"
$repo = ($pin | Where-Object { $_ -match '^repo=' }) -replace '^repo=', '' | Select-Object -First 1
$commit = ($pin | Where-Object { $_ -match '^commit=' }) -replace '^commit=', '' | Select-Object -First 1
if (-not $repo) { Fail "whisper.pin has no repo= line" }
if ($commit -notmatch '^[0-9a-f]{40}$') { Fail "whisper.pin commit is not a 40-character sha" }

foreach ($tool in @("git", "cmake")) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { Fail "$tool not found in PATH" }
}
if (-not $CpuOnly -and -not (Get-Command "glslc" -ErrorAction SilentlyContinue)) {
    Fail "glslc not found: install the LunarG Vulkan SDK, or pass -CpuOnly"
}
if ($Jobs -le 0) { $Jobs = [Environment]::ProcessorCount }

$src = ".whisper\src"
New-Item -ItemType Directory -Force -Path $src, ".whisper\bin" | Out-Null

if (-not (Test-Path "$src\.git")) {
    Write-Host "build-whisper: cloning $repo"
    & git init -q $src
    & git -C $src remote add origin $repo
}
& git -C $src remote set-url origin $repo
& git -C $src cat-file -e "$commit^{commit}" 2>$null
if ($LASTEXITCODE -ne 0) {
    Write-Host "build-whisper: fetching $commit"
    & git -C $src fetch --depth 1 origin $commit
    if ($LASTEXITCODE -ne 0) { Fail "could not fetch $commit from $repo" }
}
& git -C $src checkout -q --force --detach $commit
& git -C $src clean -qfdx -e build-cpu -e build-vulkan

function Build($name, $vulkan) {
    $dir = ".whisper\$name"
    Write-Host "build-whisper: configuring $name (GGML_VULKAN=$vulkan)"
    # GGML_NATIVE=OFF on purpose: a binary built for this machine's instruction set crashes on
    # any older one, which is not something to ship.
    & cmake -S $src -B $dir `
        -DCMAKE_BUILD_TYPE=Release `
        -DBUILD_SHARED_LIBS=OFF `
        -DGGML_NATIVE=OFF `
        -DGGML_VULKAN=$vulkan `
        -DWHISPER_BUILD_EXAMPLES=ON `
        -DWHISPER_BUILD_TESTS=OFF `
        -DWHISPER_BUILD_SERVER=OFF
    if ($LASTEXITCODE -ne 0) { Fail "cmake configure failed for $name" }
    Write-Host "build-whisper: building $name with $Jobs jobs"
    & cmake --build $dir --config Release --target whisper-cli -j $Jobs
    if ($LASTEXITCODE -ne 0) { Fail "build failed for $name" }
}

function InstallBinary($dir, $target) {
    foreach ($candidate in @("$dir\bin\Release\whisper-cli.exe", "$dir\bin\whisper-cli.exe")) {
        if (Test-Path $candidate) {
            Copy-Item -Force $candidate ".whisper\bin\$target.exe"
            return
        }
    }
    Fail "no whisper-cli.exe produced under $dir\bin"
}

Build "build-cpu" "OFF"
InstallBinary ".whisper\build-cpu" "whisper-cli-cpu"
if (-not $CpuOnly) {
    Build "build-vulkan" "ON"
    InstallBinary ".whisper\build-vulkan" "whisper-cli"
}

Write-Host "build-whisper: done, from commit $commit"
Get-ChildItem ".whisper\bin"
