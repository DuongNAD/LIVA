# run-migration.ps1
# Utility to execute, check, or audit Sequelize Postgres migrations locally.

param(
    [switch]$Status,
    [switch]$Apply
)

# Move to the server directory if executed from root
$Cwd = Get-Location
if ($Cwd.Path.EndsWith("project-name")) {
    $ServerPath = Join-Path $Cwd.Path "server"
} else {
    $ServerPath = $Cwd.Path
}

Write-Host "📂 Target directory: $ServerPath" -ForegroundColor Cyan

if ($Status) {
    Write-Host "📋 Fetching applied and pending migrations..." -ForegroundColor Yellow
    npx tsx "$ServerPath\scripts\migrate.ts" --status
}
elseif ($Apply) {
    Write-Host "🚀 Running all pending schema migrations..." -ForegroundColor Yellow
    npx tsx "$ServerPath\scripts\migrate.ts"
}
else {
    Write-Host "Antigravity-Vibe Database Migration Runner Helper" -ForegroundColor Cyan
    Write-Host "--------------------------------------"
    Write-Host "Usage:"
    Write-Host "  .\run-migration.ps1 -Status   : List applied and pending migrations"
    Write-Host "  .\run-migration.ps1 -Apply    : Apply all pending migrations to the local database"
}
