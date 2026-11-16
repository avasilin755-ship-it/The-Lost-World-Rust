param(
    [string]$Root = "$HOME\Games\DieVolker_Original",
    [ValidateSet("Sajikis.mdb","Pimmons.mdb","Amazons.mdb","Tutorials.mdb",
                 "AmazonsAO.mdb","PimmonsAO.mdb","SajikisAO.mdb")]
    [string]$Campaign = "Sajikis.mdb",
    [ValidateRange(0,50)]
    [int]$Mission = 0
)

$ErrorActionPreference = "Stop"

Write-Host "THE LOST WORLD - RUST EDITION" -ForegroundColor Yellow
Write-Host "Checking original game resources (no installers are executed)..." -ForegroundColor Cyan

$gd = Get-ChildItem (Join-Path $Root "GameAssets\Base") -Recurse -File -Filter "dv.gd" -ErrorAction SilentlyContinue |
    Select-Object -First 1
if (-not $gd) {
    throw "Cannot find dv.gd. Extract the base InstallShield DATA1.CAB into the Base assets directory."
}

$data = $gd.DirectoryName
$addon = Join-Path $Root "GameAssets\MissionPack"

$needed = @(
    "dv.gd", "dv.OD", "dv.odb", "wiese.os", "weg.os",
    "gelbgras.os", "gruenwald.os", "waldboden.os", "rock.os",
    "schnee.os", "wasser1.os", "wasser2.os", "wasser3.os",
    "wasser4.os", "transition1.os", "transition2.os",
    "transition3.os", "transition4.os"
)
$missing = @($needed | Where-Object { -not (Test-Path (Join-Path $data $_)) })
if ($missing.Count -gt 0) {
    Write-Host "Missing files in $data" -ForegroundColor Red
    $missing | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    throw "Extract missing original files before launching. See rust/game-client/README.md."
}

if (-not (Test-Path (Join-Path $data $Campaign))) {
    throw "Campaign database '$Campaign' is not in '$data'. Gold add-on missions also require extracting their MDB and DVM files locally."
}
if (-not (Test-Path (Join-Path $addon "ADDON.GD"))) {
    throw "Missing ADDON.GD; extract ADDON.* from Mission Pack setup.EXE into '$addon'."
}

Write-Host "Data: $data" -ForegroundColor Green
Write-Host "Mission Pack: $addon" -ForegroundColor Green
Write-Host "Campaign: $Campaign (mission $Mission)" -ForegroundColor Green
Write-Host "Starting the standalone Rust client..." -ForegroundColor Yellow

cargo run --release --manifest-path rust/game-client/Cargo.toml -- "$data" "$addon" "$Campaign" "$Mission"
if ($LASTEXITCODE -ne 0) {
    throw "Rust client exited with code $LASTEXITCODE"
}
