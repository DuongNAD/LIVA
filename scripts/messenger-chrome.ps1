# Mở Chrome cho LIVA lái Messenger, trong một profile RIÊNG.
#
# Chạy:  powershell -ExecutionPolicy Bypass -File scripts\messenger-chrome.ps1
#
# ── Vì sao phải là profile riêng ──────────────────────────────────────────────
#
# Từ Chrome 136, `--remote-debugging-port` bị TỪ CHỐI trên thư mục profile mặc
# định. Đó là bản vá bảo mật của Google: một cổng debug mở trên profile bạn đang
# dùng nghĩa là mọi tiến trình trên máy đọc được cookie của mọi trang bạn đăng
# nhập. Nên "gắn vào Chrome đang mở" là việc không làm được nữa, không phải là
# việc chưa ai làm. Máy này đang chạy Chrome 150.
#
# Hệ quả kéo theo: profile mới thì chưa đăng nhập Facebook. Bạn phải tự đăng
# nhập MỘT LẦN trong cửa sổ này. Sau đó cookie nằm trong profile và còn mãi.
#
# ── LIVA không bao giờ chạm vào mật khẩu ──────────────────────────────────────
#
# Không có dòng nào trong repo này nhập mật khẩu Facebook, và sẽ không có. Ngoài
# lý do an toàn (WebSocket 8002 chưa có xác thực — mục C1 trong
# docs/03-danh-gia/02-no-ky-thuat-va-rui-ro.md), nó còn đúng hơn về kỹ thuật:
# đăng nhập tự động gần như chắc chắn kích hoạt checkpoint 2FA/xác minh thiết
# bị, còn cookie sẵn thì không.
#
# ── Điều phải biết trước khi dùng ─────────────────────────────────────────────
#
# Meta CẤM tự động hoá trong điều khoản. Rủi ro khoá tài khoản là thật. Tài
# khoản mới lập càng dễ ăn checkpoint. Đây là lựa chọn của người dùng, không
# phải khuyến nghị.

param(
    [int]$Port = 0,
    [string]$ProfileDir = "",
    [switch]$WhatIf
)

$ErrorActionPreference = "Stop"

if ($Port -eq 0) {
    if ($env:LIVA_MESSENGER_CDP_PORT) { $Port = [int]$env:LIVA_MESSENGER_CDP_PORT } else { $Port = 9222 }
}
if ([string]::IsNullOrWhiteSpace($ProfileDir)) {
    $ProfileDir = Join-Path $env:LOCALAPPDATA "liva-messenger-profile"
}

$chrome = @(
    (Join-Path $env:ProgramFiles "Google\Chrome\Application\chrome.exe"),
    (Join-Path ${env:ProgramFiles(x86)} "Google\Chrome\Application\chrome.exe"),
    (Join-Path $env:LOCALAPPDATA "Google\Chrome\Application\chrome.exe")
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $chrome) {
    throw "Khong tim thay chrome.exe. Cai Chrome, hoac sua duong dan trong script nay."
}

# Cổng đã có người giữ: nếu là chính profile này thì thôi, còn là ai khác thì
# dừng — cướp cổng của tiến trình khác là cách hỏng khó tìm nhất.
$dangGiu = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
if ($dangGiu) {
    $chu = Get-Process -Id $dangGiu[0].OwningProcess -ErrorAction SilentlyContinue
    if ($chu -and $chu.ProcessName -eq "chrome") {
        Write-Host "Cong $Port da co Chrome giu (PID $($chu.Id)). Dung lai ban dang chay, khong mo them."
        Write-Host "Kiem tra bang lenh 'messenger:status' qua WebSocket 8002."
        exit 0
    }
    throw "Cong $Port dang bi '$($chu.ProcessName)' (PID $($dangGiu[0].OwningProcess)) giu. Chon cong khac bang -Port."
}

# Chi DO xem profile da co chua. Viec TAO no doi toi sau nhanh -WhatIf: ban dau
# tao ngay o day, nen mot lan chay thu -WhatIf cung tao thu muc, va lan chay that
# dau tien bao "profile da ton tai" — dung cau khien nguoi dung tuong minh da
# dang nhap roi.
$daCo = Test-Path $ProfileDir

$args = @(
    "--remote-debugging-port=$Port",
    "--user-data-dir=`"$ProfileDir`"",
    # Không dùng chung tiến trình với Chrome đang chạy của bạn.
    "--no-first-run",
    "--no-default-browser-check",
    "https://www.messenger.com"
)

Write-Host "Chrome     : $chrome"
Write-Host "Profile    : $ProfileDir$(if ($daCo) { ' (da co)' } else { ' (moi tao)' })"
Write-Host "Debug port : $Port"

if ($WhatIf) {
    Write-Host "`n[WhatIf] Se chay: $chrome $($args -join ' ')"
    Write-Host "[WhatIf] Khong tao thu muc profile, khong mo Chrome."
    exit 0
}

if (-not $daCo) { New-Item -ItemType Directory -Force -Path $ProfileDir | Out-Null }
Start-Process -FilePath $chrome -ArgumentList $args | Out-Null

Write-Host ""
if ($daCo) {
    Write-Host "Profile da ton tai — neu lan truoc da dang nhap thi lan nay vao thang."
} else {
    Write-Host "BUOC TIEP THEO LA CUA BAN: tu dang nhap Facebook trong cua so vua mo."
    Write-Host "LIVA khong nhap mat khau ho ban, va khong doc mat khau cua ban."
}
Write-Host "Xong roi thi goi lenh 'messenger:status' de LIVA tu kiem tra."
