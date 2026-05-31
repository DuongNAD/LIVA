# ================================================================
# consolidate_docs.ps1 — Markdown Consolidator for AI Context
# ================================================================
# Purpose: Consolidates all .md files in the workspace into a single
# markdown file. This consolidated file can be uploaded to:
#   - Google Gemini Gems (custom AI agent with full repo knowledge)
#   - Any AI assistant with file upload support
#   - RAG pipelines or embedding systems
#
# Use Case: Upload the output to a Gemini Gem so you can ask
# questions about your entire repo from the Google Gemini app
# on your phone — even at night from the couch. 🛋️
#
# Usage: .\consolidate_docs.ps1
# ================================================================

$rootFolder = Get-Location
$outputFile = Join-Path $rootFolder "consolidated_docs.md"

# Target folder for the consolidated output (optional: auto-move)
# Uncomment and set your Google Drive path to auto-move the file:
# $moveToFolder = "G:\My Drive\ai-context\"

# ── Folders to exclude ──────────────────────────────────────────
$excludeFolders = @(
    "node_modules",
    ".git",
    ".gemini",
    ".antigravitycli",
    ".vscode",
    "dist",
    "build",
    "logs",
    "temp",
    "archive",
    "test-results",
    "coverage"
)

# ── File patterns to exclude (case-insensitive, supports wildcards) ──
$excludeFilePatterns = @(
    "*license*",
    "*contributing*",
    "*authors*",
    "*code_of_conduct*",
    "security.md",
    "pull_request_template.md",
    "issue_template.md"
)

# ── Files to force-include (even if in an excluded folder) ──
$forceIncludeFiles = @(
    # "node_modules\some-important-package\ARCHITECTURE.md"
)

# ── Step 1: Prepare output file ─────────────────────────────────
if (Test-Path $outputFile) {
    Clear-Content -Path $outputFile
    Write-Host "Cleared existing file: $outputFile" -ForegroundColor Cyan
} else {
    New-Item -ItemType File -Path $outputFile -Force | Out-Null
}

# ── Step 2: Collect and filter .md files ────────────────────────
$mdFiles = Get-ChildItem -Path $rootFolder -Filter *.md -Recurse | Where-Object {
    $filePath = $_.FullName
    $fileName = $_.Name
    $isExcluded = $false
    $isForceIncluded = $false

    # Check force-include list
    foreach ($forcePath in $forceIncludeFiles) {
        if ($filePath -like "*\$forcePath" -or $filePath -eq $forcePath) {
            $isForceIncluded = $true
            break
        }
    }

    if (-not $isForceIncluded) {
        # Check excluded folders
        foreach ($folder in $excludeFolders) {
            if ($filePath -like "*\$folder\*") { $isExcluded = $true; break }
        }

        # Check excluded file patterns
        if (-not $isExcluded) {
            foreach ($pattern in $excludeFilePatterns) {
                if ($fileName -like $pattern) { $isExcluded = $true; break }
            }
        }
    }

    # Keep if force-included OR (not excluded and not the output file)
    $isForceIncluded -or (-not $isExcluded -and ($filePath -ne $outputFile))
}

# ── Step 3: Consolidate files ───────────────────────────────────
foreach ($file in $mdFiles) {
    Write-Host "Adding: $($file.Name)" -ForegroundColor Yellow

    # Add filename as H1 header
    Add-Content -Path $outputFile -Value "# $($file.Name)`n"

    # Insert file content and separator
    Get-Content -Path $file.FullName | Add-Content -Path $outputFile
    Add-Content -Path $outputFile -Value "`n---`n"
}

Write-Host "`nDone! Consolidated $($mdFiles.Count) files into: $outputFile" -ForegroundColor Green

# ── Step 4 (Optional): Move to target folder ───────────────────
# Uncomment to auto-move to Google Drive or another location:
# if ($moveToFolder) {
#     Write-Host "Moving consolidated file to: $moveToFolder" -ForegroundColor Cyan
#     Move-Item -Path $outputFile -Destination $moveToFolder -Force
#     Write-Host "File moved successfully." -ForegroundColor Green
# }
