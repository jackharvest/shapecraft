# Makes the Windows downloads from a release build: a portable zip (the app, the CLI and every DLL they load that
# Windows doesn't ship: OpenCASCADE, TBB, the VC runtime…) and an installer built from the same folder with Inno Setup.
#
#   cargo build --release -p shapecraft -p shapecraft-cli
#   pwsh packaging/windows/bundle.ps1          (in a Visual Studio developer shell, for dumpbin)
#   → target\shapecraft-<version>-windows-x64-portable.zip and target\shapecraft-<version>-windows-x64-setup.exe
#
# DLLs are looked up in $env:OCCT_BIN (default: $env:CONDA_PREFIX\Library\bin), then $env:CONDA_PREFIX; anything not
# found there is treated as part of Windows. The installer needs Inno Setup 6 (iscc on PATH or in Program Files).
$ErrorActionPreference = 'Stop'
$root = Resolve-Path "$PSScriptRoot\..\.."
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "$root\target" }
$release = "$target\release"
$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$name = "shapecraft-$version-windows-x64"
$stage = "$target\windows-bundle\ShapeCraft"

$search = @()
if ($env:OCCT_BIN) { $search += $env:OCCT_BIN }
if ($env:CONDA_PREFIX) { $search += "$env:CONDA_PREFIX\Library\bin"; $search += $env:CONDA_PREFIX }
if (-not $search) { throw 'Set OCCT_BIN or activate the conda environment with OCCT.' }

Remove-Item -Recurse -Force "$target\windows-bundle" -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
foreach ($exe in 'shapecraft.exe', 'shapecraft-cli.exe') {
  if (-not (Test-Path "$release\$exe")) { throw "No release build: $release\$exe" }
  Copy-Item "$release\$exe" $stage
}

# The DLL closure: what each file imports, found in the search folders, copied, and walked in turn.
$queue = [System.Collections.Generic.Queue[string]]::new()
$queue.Enqueue("$stage\shapecraft.exe"); $queue.Enqueue("$stage\shapecraft-cli.exe")
$seen = @{}
while ($queue.Count) {
  $file = $queue.Dequeue()
  $imports = & dumpbin /nologo /dependents $file | Where-Object { $_ -match '^\s+\S+\.dll\s*$' } | ForEach-Object { $_.Trim() }
  foreach ($dll in $imports) {
    $key = $dll.ToLowerInvariant()
    if ($seen.ContainsKey($key)) { continue }
    $seen[$key] = $true
    $src = $search | ForEach-Object { Join-Path $_ $dll } | Where-Object { Test-Path $_ } | Select-Object -First 1
    if ($src) { Copy-Item $src $stage; $queue.Enqueue("$stage\$dll") }
  }
}
$count = (Get-ChildItem "$stage\*.dll").Count
Write-Host "$count DLLs bundled"

foreach ($f in 'LICENSE-MIT', 'LICENSE-APACHE', 'NOTICE', 'DISCLAIMER.md') { if (Test-Path "$root\$f") { Copy-Item "$root\$f" $stage } }
Copy-Item "$root\assets\icon\shapecraft.ico" $stage

# Both programs must start with only the folder's DLLs: the CLI is enough to load all of OCCT.
$env:PATH = "$env:SystemRoot\system32;$env:SystemRoot"
& "$stage\shapecraft-cli.exe" commands | Out-Null
if ($LASTEXITCODE -ne 0) { throw "shapecraft-cli.exe didn't start from the bundle folder ($LASTEXITCODE)" }

$zip = "$target\$name-portable.zip"
Remove-Item $zip -ErrorAction SilentlyContinue
Compress-Archive -Path $stage -DestinationPath $zip -CompressionLevel Optimal
Write-Host $zip

$iscc = (Get-Command iscc -ErrorAction SilentlyContinue).Source
if (-not $iscc) { $iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" }
if (-not (Test-Path $iscc)) { Write-Warning 'Inno Setup not found: no installer made'; exit 0 }
& $iscc /Qp "/DVersion=$version" "/DStage=$stage" "/DOutDir=$target" "/DOutName=$name-setup" "$PSScriptRoot\shapecraft.iss"
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed ($LASTEXITCODE)" }
Write-Host "$target\$name-setup.exe"
