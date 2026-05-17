import { invoke } from "@tauri-apps/api/core";
import { Stronghold, Location } from '@tauri-apps/plugin-stronghold';

let greetMsgEl: HTMLElement | null;

async function testStronghold() {
  if (!greetMsgEl) return;
  try {
    greetMsgEl.innerHTML = "Khởi tạo Két Sắt Stronghold...<br>";

    // Initialize Stronghold
    const vaultPath = "C:\\Users\\Admin\\AppData\\Local\\liva_vault_test.app";
    const password = "super-secret-password-from-keyring";
    
    greetMsgEl.innerHTML += `Đang nạp Két Sắt từ: ${vaultPath}...<br>`;
    // We create a new stronghold instance
    const stronghold = await Stronghold.load(vaultPath, password);
    greetMsgEl.innerHTML += "Két sắt load thành công...<br>";
    
    let client;
    try {
      client = await stronghold.loadClient("liva_client");
      greetMsgEl.innerHTML += "Client load thành công...<br>";
    } catch {
      client = await stronghold.createClient("liva_client");
      greetMsgEl.innerHTML += "Client create thành công...<br>";
    }
    
    const store = client.getStore();

    greetMsgEl.innerHTML += "Két sắt mở thành công!<br>";

    // Write a mock Zalo Token
    const mockToken = "zalo_oa_xyz_123456";
    // Convert string to array of bytes
    const encoder = new TextEncoder();
    const tokenBytes = Array.from(encoder.encode(mockToken));
    
    greetMsgEl.innerHTML += "Đang ghi dữ liệu vào Store...<br>";
    await store.insert("ZALO_OA_TOKEN", tokenBytes);
    greetMsgEl.innerHTML += "Ghi vào Store (Memory) thành công!<br>";

    // Read it back
    const retrieved = await store.get("ZALO_OA_TOKEN");
    if (retrieved) {
      const decoder = new TextDecoder();
      const tokenStr = decoder.decode(new Uint8Array(retrieved));
      greetMsgEl.innerHTML += `Đọc lại từ két sắt: ${tokenStr}<br>`;
      greetMsgEl.innerHTML += "<br><span style='color: #00ff00; font-weight: bold; font-size: 18px;'>[PoC THÀNH CÔNG] - Sẵn sàng thay thế electron.safeStorage!</span>";
    } else {
      greetMsgEl.innerHTML += "<span style='color: red'>[PoC THẤT BẠI] - Không thể đọc lại Token!</span>";
    }

  } catch (err: any) {
    const msg = typeof err === 'string' ? err : (err.message || JSON.stringify(err));
    greetMsgEl.innerHTML += `<br><span style='color: red'>LỖI: ${msg}</span>`;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  greetMsgEl = document.querySelector("#greet-msg");
  
  // Set transparent background to test Ghost Mode
  document.body.style.background = "rgba(0, 0, 0, 0.1)"; // Almost transparent
  
  // Run test
  testStronghold();
});
