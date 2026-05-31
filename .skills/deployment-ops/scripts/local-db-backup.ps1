# local-db-backup.ps1
# PowerShell helper utility to manually invoke PostgreSQL pg_dump backups locally.

param(
    [string]$Name = "manual_snapshot",
    [switch]$Seed
)

# Resolve backups directory path
$Cwd = Get-Location
$BackupsPath = ""

if ($Cwd.Path.EndsWith("project-name")) {
    $BackupsPath = Join-Path $Cwd.Path "backups"
} else {
    $BackupsPath = Join-Path (Split-Path $Cwd.Path) "backups"
}

# Load database environment variables from parent .env if present
$DbUser = "postgres"
$DbName = "antigravity-vibe"
$DbPass = "postgres"
$DbHost = "localhost"
$DbPort = "5432"

if (Test-Path "$Cwd\.env") {
    $EnvContent = Get-Content "$Cwd\.env"
    foreach ($Line in $EnvContent) {
        if ($Line -match "^DB_USER=(.*)$") { $DbUser = $Matches[1] }
        if ($Line -match "^DB_NAME=(.*)$") { $DbName = $Matches[1] }
        if ($Line -match "^DB_PASS=(.*)$") { $DbPass = $Matches[1] }
        if ($Line -match "^DB_HOST=(.*)$") { $DbHost = $Matches[1] }
        if ($Line -match "^DB_PORT=(.*)$") { $DbPort = $Matches[1] }
    }
}

# Construct Backup Filename
$Timestamp = (Get-Date).ToString("yyyy-MM-ddTHH-mm-ss")
$SanitizedName = $Name -replace '[^a-zA-Z0-9-_]', '_'
$Filename = "${Timestamp}_${SanitizedName}.dump"
$Filepath = Join-Path $BackupsPath $Filename

Write-Host "📦 Preparing manual database snapshot..." -ForegroundColor Cyan
Write-Host "   Database : $DbName" -ForegroundColor Gray
Write-Host "   User     : $DbUser" -ForegroundColor Gray
Write-Host "   Path     : $Filepath" -ForegroundColor Gray

# Set Postgres password env var for silent spawn
$env:PGPASSWORD = $DbPass

# Proactively run pg_dump with Custom Format (-Fc)
& pg_dump -h $DbHost -p $DbPort -U $DbUser -F c -v -f $Filepath $DbName

if ($LASTEXITCODE -eq 0) {
    Write-Host "✅ Database snapshot completed successfully!" -ForegroundColor Green
    
    if ($Seed) {
        $SeedPath = Join-Path $BackupsPath "antigravity-vibe_seed.dump"
        Copy-Item $Filepath $SeedPath -Force
        Write-Host "✅ Copied dump to seed path: $SeedPath" -ForegroundColor Green
    }
} else {
    Write-Host "❌ pg_dump exited with error code: $LASTEXITCODE" -ForegroundColor Red
}

# Clear password env var for safety
$env:PGPASSWORD = $null
