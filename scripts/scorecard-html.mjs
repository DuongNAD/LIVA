#!/usr/bin/env node

// Sinh trang phiếu chấm từ bản đo của `scorecard.mjs`.
//
// Tách khỏi scorecard.mjs vì hai việc khác nhau: một bên ĐO, một bên TRÌNH BÀY.
// Gộp lại thì mỗi lần chỉnh màu là đụng vào file chứa luật chấm điểm.
//
// Trang này KHÔNG tự đọc được trạng thái dự án khi mở — artifact chạy trên
// claude.ai, không với tới máy người dùng. Nên số liệu được nhúng thẳng vào HTML
// lúc sinh, và trang hiện rõ mốc thời gian + sha để người đọc biết mình đang
// nhìn ảnh chụp lúc nào. Muốn tươi thì chạy lại script rồi publish đè.

const esc = (s) => String(s)
  .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;')

const NGAY = (iso) => {
  if (!iso) return 'chưa từng'
  const d = new Date(iso)
  return `${d.getDate()}/${d.getMonth() + 1}/${d.getFullYear()}`
}

/** "hôm nay" / "3 ngày trước" — tuổi của số đo đắt, thứ người đọc cần biết nhất. */
const TUOI = (iso) => {
  if (!iso) return 'chưa đo lần nào'
  const ngay = Math.floor((Date.now() - new Date(iso).getTime()) / 86400000)
  if (ngay <= 0) return 'đo hôm nay'
  if (ngay === 1) return 'đo hôm qua'
  return `đo ${ngay} ngày trước`
}

export function sinhHtml(ban) {
  const { soDo: m, diem } = ban
  const phanTram = Math.round((diem.tong / diem.tran) * 100)

  const hangTieuChi = diem.tieuChi.map((tc) => {
    const yeu = tc.tyLe < 0.75
    const chiBao = tc.chiBao.map((cb) => `
        <li class="cb${cb.diem < cb.tran ? ' cb-hut' : ''}">
          <span class="cb-d mono">${cb.diem}/${cb.tran}</span>
          <span class="cb-t">${esc(cb.ten)}</span>
          <span class="cb-v">${esc(cb.hien)}</span>
          <span class="cb-n mono">${cb.loai === 'tay' ? `tay · ${NGAY(cb.reviewedAt)}` : 'đo'}</span>
        </li>`).join('')
    return `
      <article class="crit"${yeu ? ' data-flag' : ''}>
        <div class="crit-top">
          <span class="crit-name">${esc(tc.ten)}</span>
          <span class="crit-score mono">${tc.diem}<s> / ${tc.tran}</s></span>
        </div>
        <div class="bar"><i style="width:${(tc.tyLe * 100).toFixed(1)}%"></i></div>
        <ul class="cbs">${chiBao}</ul>
      </article>`
  }).join('')

  return `<title>Phiếu chấm LIVA</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Newsreader:opsz,wght@6..72,400;6..72,500;6..72,600&family=IBM+Plex+Mono:wght@400;500;600&display=swap">
<style>
:root{
  --ground:#F6F8F7; --panel:#FFF; --rule:#DCE4E1; --track:#E4EAE8;
  --ink:#141A1B; --ink-2:#3C4A4A; --muted:#6B7A79;
  --accent:#0E6B60; --accent-soft:#0E6B601a; --flag:#8A5D0B; --flag-soft:#8A5D0B12;
}
@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){
  --ground:#0D1112; --panel:#141A1B; --rule:#26302F; --track:#232C2C;
  --ink:#E7EEEC; --ink-2:#B4C2BF; --muted:#7E8E8C;
  --accent:#43BCA9; --accent-soft:#43BCA91f; --flag:#D9A544; --flag-soft:#D9A54414;
}}
:root[data-theme="dark"]{
  --ground:#0D1112; --panel:#141A1B; --rule:#26302F; --track:#232C2C;
  --ink:#E7EEEC; --ink-2:#B4C2BF; --muted:#7E8E8C;
  --accent:#43BCA9; --accent-soft:#43BCA91f; --flag:#D9A544; --flag-soft:#D9A54414;
}
*{box-sizing:border-box}
body{background:var(--ground);color:var(--ink);margin:0;
  font-family:"Newsreader",Georgia,serif;font-size:17px;line-height:1.6;
  padding:clamp(20px,4vw,60px) clamp(16px,4vw,40px) 90px;-webkit-font-smoothing:antialiased}
.sheet{max-width:960px;margin:0 auto;display:flex;flex-direction:column;gap:42px}
.mono{font-family:"IBM Plex Mono",ui-monospace,Menlo,monospace}
.lab{font-family:"IBM Plex Mono",Menlo,monospace;font-size:11px;font-weight:500;
  letter-spacing:.13em;text-transform:uppercase;color:var(--muted)}
.head{display:flex;flex-direction:column;gap:18px}
.head-rule{height:2px;background:var(--ink);opacity:.86}
.title{font-size:clamp(38px,7vw,58px);font-weight:600;line-height:1.02;
  letter-spacing:-.022em;margin:0;text-wrap:balance}
.total{display:flex;align-items:flex-end;gap:16px;flex-wrap:wrap;padding:18px 0 2px}
.total-num{font-family:"IBM Plex Mono",monospace;font-weight:600;
  font-size:clamp(62px,13vw,110px);line-height:.86;letter-spacing:-.045em;
  color:var(--accent);font-variant-numeric:tabular-nums}
.total-den{font-family:"IBM Plex Mono",monospace;font-size:clamp(20px,4vw,28px);
  color:var(--muted);line-height:1;padding-bottom:6px}
.total-note{flex:1;min-width:230px;color:var(--muted);font-size:15px;padding-bottom:5px}
h2.sec{font-size:13px;font-family:"IBM Plex Mono",monospace;font-weight:600;
  letter-spacing:.13em;text-transform:uppercase;color:var(--muted);
  margin:0 0 14px;padding-bottom:9px;border-bottom:1px solid var(--rule)}
.crits{display:flex;flex-direction:column;gap:2px}
.crit{background:var(--panel);border:1px solid var(--rule);padding:17px 20px;
  display:flex;flex-direction:column;gap:11px;transition:border-color .16s}
.crit:first-of-type{border-radius:8px 8px 0 0}
.crit:last-of-type{border-radius:0 0 8px 8px}
.crit:hover{border-color:var(--accent)}
.crit[data-flag]{background:var(--flag-soft)}
.crit-top{display:flex;align-items:baseline;gap:14px;flex-wrap:wrap}
.crit-name{font-size:20px;font-weight:500;letter-spacing:-.01em;flex:1;min-width:160px}
.crit-score{font-variant-numeric:tabular-nums;font-size:19px;font-weight:600}
.crit-score s{text-decoration:none;color:var(--muted);font-weight:400;font-size:15px}
.bar{height:9px;background:var(--track);border-radius:5px;overflow:hidden;position:relative}
.bar i{position:absolute;inset:0 auto 0 0;background:var(--accent);border-radius:0 4px 4px 0}
.crit[data-flag] .bar i{background:var(--flag)}
.cbs{list-style:none;margin:2px 0 0;padding:0;display:flex;flex-direction:column;gap:1px}
.cb{display:grid;grid-template-columns:52px minmax(150px,1.1fr) minmax(120px,1.4fr) auto;
  gap:12px;align-items:baseline;padding:5px 0;font-size:14.5px;
  border-top:1px solid var(--rule);color:var(--ink-2)}
.cb-d{font-variant-numeric:tabular-nums;font-size:13px;font-weight:600;color:var(--muted)}
.cb-hut .cb-d{color:var(--flag)}
.cb-t{color:var(--ink)}
.cb-v{color:var(--muted)}
.cb-n{font-size:10.5px;letter-spacing:.09em;text-transform:uppercase;color:var(--muted);
  opacity:.75;white-space:nowrap;text-align:right}
.foot{border-top:1px solid var(--rule);padding-top:18px;display:flex;
  flex-direction:column;gap:8px}
.foot p{margin:0;font-size:14px;color:var(--muted);max-width:78ch}
.foot code{font-family:"IBM Plex Mono",monospace;font-size:12.5px;background:var(--track);
  padding:1px 5px;border-radius:3px;color:var(--ink-2)}
.stale{border-left:3px solid var(--flag);background:var(--flag-soft);padding:12px 16px;
  border-radius:0 8px 8px 0;font-size:14.5px;color:var(--ink-2)}
@media (max-width:640px){
  .cb{grid-template-columns:46px 1fr;gap:4px 10px}
  .cb-v{grid-column:2}.cb-n{grid-column:2;text-align:left}
}
@media (prefers-reduced-motion:reduce){*{transition:none!important}}
</style>

<div class="sheet">
  <header class="head">
    <div class="head-rule"></div>
    <div>
      <p class="lab" style="margin:0 0 10px">Phiếu chấm · ${NGAY(ban.sinhLuc)} · <span class="mono">${esc(m.sha ?? '?')}</span> trên nhánh ${esc(m.nhanh ?? '?')}</p>
      <h1 class="title">LIVA</h1>
    </div>
    <div class="total">
      <span class="total-num">${diem.tong}</span>
      <span class="total-den">/ ${diem.tran}</span>
      <span class="total-note">${m.dongRust.toLocaleString('vi-VN')} dòng Rust · ${m.dongWeb.toLocaleString('vi-VN')} dòng TS/Vue · ${m.buocCI} bước CI trên ${m.jobCI} job · ${m.fileTestTichHop} file test tích hợp</span>
    </div>
    <p class="lab" style="margin:0">${Math.round(diem.tyLeTuDo * 100)}% trọng số là chỉ báo đo bằng máy — phần còn lại chấm tay, có ghi ngày rà</p>
  </header>

  <section>
    <h2 class="sec">Bảy tiêu chí, ${diem.tieuChi.reduce((s, t) => s + t.chiBao.length, 0)} chỉ báo</h2>
    <div class="crits">${hangTieuChi}
    </div>
  </section>

  ${m.doDacDatLuc ? '' : `<div class="stale"><b>Chưa từng chạy <span class="mono">--full</span>.</b> Số test và số cảnh báo clippy cần biên dịch nên mặc định không đo; hai chỉ báo đó đang tính 0 điểm. Chạy <span class="mono">node scripts/scorecard.mjs --full</span> để có điểm thật.</div>`}

  <footer class="foot">
    <p><b>Cách chấm.</b> Mỗi tiêu chí là tổng các chỉ báo con, mỗi chỉ báo quy ra điểm theo ngưỡng viết thẳng trong <code>scripts/scorecard.mjs</code>. Đổi mã nguồn thì điểm tự đổi — không có con số nào gõ tay vào trang này.</p>
    <p>Chỉ báo gắn nhãn <b>đo</b> lấy từ máy lúc sinh trang. Nhãn <b>tay</b> là phán đoán người, kèm ngày rà gần nhất để bạn tự trừ hao — trộn hai loại mà không dán nhãn là cách một phiếu chấm nói dối êm nhất.</p>
    <p>Số test và clippy: ${TUOI(m.doDacDatLuc)}. Sinh lúc ${NGAY(ban.sinhLuc)} từ <code>${esc(m.sha ?? '?')}</code>. Trang này là ảnh chụp — nó không với tới máy bạn được, nên muốn tươi thì chạy lại script và publish đè.</p>
  </footer>
</div>
`
}
