[System.Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$headers = @{ "Content-Type" = "application/json; charset=utf-8" }
$body = @{
    model = "Qwen2.5-7B-Instruct-Q8_0"
    messages = @(
        @{ role = "user"; content = "Vietnamese Text Test: Tốt" }
    )
} | ConvertTo-Json -Depth 5 -Compress

$bytes = [System.Text.Encoding]::UTF8.GetBytes($body)

Invoke-RestMethod -Uri "http://127.0.0.1:8000/v1/chat/completions" -Method Post -Headers $headers -Body $bytes
