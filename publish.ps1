param(
  [switch]$Install,
  [switch]$Pack
)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$PluginId = "dev.flowingspdg.multiobs.rust"
$PluginDir = Join-Path $Root "$PluginId.sdPlugin"
Set-Location $Root
cargo run -p multiobs-plugin --bin typegen
Push-Location (Join-Path $Root "pi")
if (-not (Test-Path "node_modules")) { npm install }
npm run build
Pop-Location
rustup target add x86_64-pc-windows-msvc
cargo build -p multiobs-plugin --release --bin multiobs_plugin --target x86_64-pc-windows-msvc
$Dest = Join-Path $PluginDir "bin\win-x64"
New-Item -ItemType Directory -Force -Path $Dest | Out-Null
Copy-Item (Join-Path $Root "target\x86_64-pc-windows-msvc\release\multiobs_plugin.exe") (Join-Path $Dest "$PluginId.exe") -Force
if ($Pack) {
  $Out = Join-Path $Root "artifacts\plugin"
  New-Item -ItemType Directory -Force -Path $Out | Out-Null
  if (Get-Command streamdeck -ErrorAction SilentlyContinue) {
    streamdeck pack $PluginDir --output $Out --force
  }
}
