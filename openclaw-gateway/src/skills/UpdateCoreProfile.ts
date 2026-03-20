import { MemoryManager } from '../MemoryManager';

export const metadata = {
    name: "update_core_profile",
    description: "Cập nhật hồ sơ tĩnh của người dùng khi có yêu cầu thay đổi (ví dụ: tuổi, nghề nghiệp, quê quán).",
    parameters: {
        type: "object",
        properties: {
            age: { type: "number", description: "Tuổi mới của người dùng" },
            profession: { type: "string", description: "Nghề nghiệp mới của người dùng" },
            location: { type: "string", description: "Quê quán / Nơi ở mới" }
        },
        required: []
    }
};

export const execute = async (args: any) => {
    // Khởi tạo MemoryManager độc lập để truy cập file user profile
    const memory = new MemoryManager('liva_core');
    await memory.updateUserProfile(args);
    return "Đã cập nhật thành công (Successfully updated)";
};
