import { execute } from '../openclaw-gateway/src/skills/ResearchIdeation';

(async () => {
    try {
        console.log("Bắt đầu kích hoạt AI Scientist để Nghiên Cứu...");
        const result = await execute({
            topic: "Cách kết hợp Alpine Linux với AI Agent",
            fileLocation: "E:/Project/LIVA/scratch_workspace"
        });
        console.log("KẾT QUẢ ĐẠT ĐƯỢC:", result);
    } catch (e) {
        console.error(e);
    }
})();
