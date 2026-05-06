import fs from 'node:fs/promises';
import path from 'node:path';

const TARGET_PATHS = [
    'src/skills',
    'src/services',
    'src/workers/WhisperWorker.ts'
];

async function getFiles(dirPath: string): Promise<string[]> {
    const stat = await fs.stat(dirPath);
    if (stat.isFile()) {
        return dirPath.endsWith('.ts') ? [dirPath] : [];
    }

    let files: string[] = [];
    const entries = await fs.readdir(dirPath, { withFileTypes: true });

    for (const entry of entries) {
        const fullPath = path.join(dirPath, entry.name);
        if (entry.isDirectory()) {
            files = files.concat(await getFiles(fullPath));
        } else if (entry.isFile() && fullPath.endsWith('.ts')) {
            files.push(fullPath);
        }
    }
    return files;
}

async function fixCatchAny() {
    console.log('🚀 Bắt đầu quét và chuẩn hóa catch (e: any)...');
    let totalFixed = 0;

    for (const targetPath of TARGET_PATHS) {
        const fullTargetPath = path.resolve(process.cwd(), targetPath);
        
        try {
            const files = await getFiles(fullTargetPath);
            
            for (const file of files) {
                let content = await fs.readFile(file, 'utf-8');
                
                // Regex tìm catch (e: any) { hoặc catch(e: any) {
                const catchRegex = /catch\s*\(\s*([a-zA-Z0-9_]+)\s*:\s*any\s*\)\s*\{/g;
                
                if (catchRegex.test(content)) {
                    // Thay thế bằng catch (e: unknown) và chèn type guard
                    const newContent = content.replace(catchRegex, (match, varName) => {
                        return `catch (${varName}: unknown) {\n            const errMsg = ${varName} instanceof Error ? ${varName}.message : String(${varName});`;
                    });

                    // Tùy chọn nâng cao: Thay thế biến varName.message thành errMsg trong file để tránh lỗi type chưa xác định
                    // (Lưu ý: regex này chỉ thay thế đơn giản, không phân tích theo context block)
                    const safeContent = newContent.replace(/e\.message/g, 'errMsg');

                    if (content !== safeContent) {
                        await fs.writeFile(file, safeContent, 'utf-8');
                        console.log(`✅ Đã chuẩn hóa: ${path.relative(process.cwd(), file)}`);
                        totalFixed++;
                    }
                }
            }
        } catch (error: unknown) {
            const errMsg = error instanceof Error ? error.message : String(error);
            console.error(`⛔ Lỗi khi duyệt đường dẫn ${targetPath}:`, errMsg);
        }
    }

    console.log(`\n🎉 Hoàn tất! Đã sửa ${totalFixed} file.`);
}

fixCatchAny().catch(console.error);
