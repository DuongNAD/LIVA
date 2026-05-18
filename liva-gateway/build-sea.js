import * as esbuild from 'esbuild';
import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function build() {
    console.log("🚀 [1/4] Bắt đầu Bundle dự án bằng esbuild...");
    
    // Đảm bảo thư mục dist tồn tại
    if (!fs.existsSync('dist')) {
        fs.mkdirSync('dist');
    }

    // 1. Bundle code
    await esbuild.build({
        entryPoints: ['src/Gateway.ts'],
        bundle: true,
        platform: 'node',
        target: 'node20',
        outfile: 'dist/bundle.js',
        format: 'cjs', // Node SEA yêu cầu chuẩn CommonJS cho đầu vào
        // BẮT BUỘC BỎ QUA NATIVE MODULES ĐỂ KHÔNG BỊ CRASH
        external: [
            '@lancedb/lancedb',
            'sqlite3',
            'esbuild',
            '@ast-grep/napi',
            'active-win',
            'clipboardy',
            'ws',
            'onnxruntime-node',
            'kokoro-js'
        ]
    });
    console.log("✅ [1/4] Hoàn tất Bundle.");

    console.log("📦 [2/4] Đóng gói Blob (Node.js SEA)...");
    try {
        execSync('node --experimental-sea-config sea-config.json', { stdio: 'inherit' });
        console.log("✅ [2/4] Hoàn tất sinh Blob.");
    } catch (e) {
        console.error("❌ [2/4] Lỗi sinh Blob:", e.message);
        process.exit(1);
    }

    console.log("💉 [3/4] Tiêm Blob vào Node Executable...");
    try {
        // Copy node.exe ra thành liva-gateway.exe
        const nodePath = process.execPath;
        const exePath = path.join('dist', 'liva-gateway.exe');
        fs.copyFileSync(nodePath, exePath);
        
        // Remove signature from copied node (Windows only)
        try {
            execSync(`signtool remove /s ${exePath}`, { stdio: 'ignore' });
        } catch (e) {
            // signtool may not be installed, ignore
        }
        
        // Postject tiêm blob
        execSync(`npx postject ${exePath} NODE_SEA_BLOB dist/sea-prep.blob --sentinel-fuse NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2`, { stdio: 'inherit' });
        console.log("✅ [3/4] Hoàn tất đóng gói .exe.");
    } catch (e) {
        console.error("❌ [3/4] Lỗi tiêm Postject:", e.message);
        process.exit(1);
    }

    console.log("📂 [4/4] Quét và Copy Native Addons hậu kỳ...");
    const nativeModulesDirs = [
        '../node_modules/@lancedb/lancedb-win32-x64-msvc',
        '../node_modules/@ast-grep/napi-win32-x64-msvc',
        '../node_modules/active-win/lib/binding/napi-6-win32-unknown-x64',
        '../node_modules/onnxruntime-node/bin/napi-v6/win32/x64'
    ];

    let copiedCount = 0;
    
    // Copy tất cả file .node tìm thấy trong các thư mục native
    for (const modDir of nativeModulesDirs) {
        const fullPath = path.join(__dirname, modDir);
        if (fs.existsSync(fullPath)) {
            const files = fs.readdirSync(fullPath);
            for (const file of files) {
                if (file.endsWith('.node') || file.endsWith('.dll')) {
                    fs.copyFileSync(path.join(fullPath, file), path.join('dist', file));
                    console.log(`   Copied: ${file}`);
                    copiedCount++;
                }
            }
        }
    }
    
    console.log(`✅ [4/4] Hoàn tất Copy ${copiedCount} file Native Addon.`);
    console.log("\n🎉 BUILD THÀNH CÔNG! File chạy: dist/liva-gateway.exe");
}

build().catch(err => {
    console.error("Fatal Build Error:", err);
    process.exit(1);
});
