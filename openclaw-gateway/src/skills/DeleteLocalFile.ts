import * as fs from 'fs/promises';
import * as path from 'path';

export const metadata = {
    name: "delete_local_file",
    description: "Xóa một tệp tin trên hệ thống (Delete a file). CẢNH BÁO: Chỉ sử dụng công cụ này khi người dùng yêu cầu xóa một cách rõ ràng.",
    parameters: {
        type: "object",
        properties: {
            filePath: {
                type: "string",
                description: "Đường dẫn tuyệt đối hoặc tương đối tới tệp tin cần xóa."
            }
        },
        required: ["filePath"]
    }
};

export const execute = async (args: { filePath: string }): Promise<string> => {
    try {
        const targetPath = path.resolve(process.cwd(), args.filePath);
        console.log(`[Skill: delete_local_file] Đang tiến hành xóa tệp (Deleting file): ${targetPath}`);
        
        await fs.unlink(targetPath);
        return `Đã xóa tệp thành công (File deleted successfully): ${targetPath}`;
    } catch (error: any) {
        return `Lỗi khi xóa tệp (File deletion error): ${error.message}`;
    }
};