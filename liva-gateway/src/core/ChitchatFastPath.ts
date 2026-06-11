import { logger } from "../utils/logger";

export class ChitchatFastPath {
    private static readonly GREETINGS_VI = [
        "Dạ em chào sếp! Chúc sếp một ngày tốt lành ạ. LIVA có thể giúp gì cho sếp ạ? 😊",
        "Chào sếp! LIVA đã sẵn sàng hỗ trợ sếp rồi đây. Hôm nay sếp cần xử lý tác vụ gì thế ạ? 🚀",
        "Dạ em kính chào sếp! Có việc gì sếp cứ giao cho em nhé. 🤖",
        "Xin chào sếp! LIVA rất vui được đồng hành cùng sếp hôm nay. Em giúp gì được cho sếp ạ? ✨"
    ];

    private static readonly GREETINGS_EN = [
        "Hello! LIVA is ready to assist you. How can I help you today? 😊",
        "Hi there! How can I help you today? Let's get things done! 🚀",
        "Greetings! LIVA is at your service. What can I do for you today? 🤖",
        "Hello! Good to see you. What's on the agenda today? ✨"
    ];

    private static readonly HEALTH_VI = [
        "Dạ em cảm ơn sếp, em lúc nào cũng khỏe và sẵn sàng chiến đấu cùng sếp rồi ạ! Sếp hôm nay thế nào rồi ạ? 😊",
        "Dạ em là trợ lý AI nên lúc nào cũng tràn đầy năng lượng sếp ơi! Sếp hôm nay có khỏe không ạ? ⚡",
        "Em khỏe lắm sếp ơi! Luôn trực chiến 24/7 để phục vụ sếp đây ạ. Chúc sếp một ngày làm việc tràn đầy năng lượng nhé! 🌸"
    ];

    private static readonly HEALTH_EN = [
        "I'm doing great, thank you! Always running at peak performance. How are you doing? 😊",
        "I'm online and ready! As an AI, I'm always energized and good to go. How is your day going? ⚡",
        "System status is 100% healthy! Ready to process your commands. How are you doing today? 🤖"
    ];

    private static readonly IDENTITY_VI = [
        "Dạ em là LIVA - trợ lý AI cá nhân tự trị của sếp, phụ trách tự động hóa công việc và hỗ trợ sếp mọi lúc mọi nơi ạ! 🤖",
        "Em là LIVA (Local Intelligent Virtual Assistant), người đồng hành và trợ thủ đắc lực giúp sếp xử lý các tác vụ từ xa ạ! 🚀",
        "Dạ em là LIVA ạ. Em được thiết kế để hỗ trợ sếp quản lý ghi chú, gửi tin nhắn, xử lý dữ liệu và chạy các lệnh tự động hóa. ⚡"
    ];

    private static readonly IDENTITY_EN = [
        "I am LIVA, your autonomous AI assistant. I'm here to help you automate tasks, manage information, and assist you with daily workflows! 🤖",
        "I'm LIVA (Local Intelligent Virtual Assistant), your personal assistant designed to help you run scripts, organize files, and handle integrations. 🚀",
        "I am LIVA. I'm your dedicated local agent, ready to take tasks off your plate. ⚡"
    ];

    private static readonly THANKS_VI = [
        "Dạ sếp đừng khách khí ạ, được hỗ trợ sếp là niềm vui của em! 😊",
        "Dạ không có chi sếp ơi! Có việc gì sếp cứ dặn em nhé. 🫡",
        "Dạ sếp quá khen rồi ạ! Em luôn sẵn sàng khi sếp cần. 🚀",
        "Dạ dạ sếp! Rất vui vì đã giúp ích được cho sếp ạ. ✨"
    ];

    private static readonly THANKS_EN = [
        "You're very welcome! Happy to help. 😊",
        "Anytime! Let me know if there's anything else you need. 🫡",
        "No problem at all! Glad I could assist. 🚀",
        "My pleasure! Let's keep moving forward. ✨"
    ];

    private static readonly GOODBYE_VI = [
        "Dạ tạm biệt sếp! Hẹn gặp lại sếp sau nhé. Em sẽ luôn túc trực ở đây khi sếp cần ạ. 😊",
        "Dạ em chào sếp! Chúc sếp nghỉ ngơi hoặc làm việc hiệu quả nhé. 🫡",
        "Tạm biệt sếp nha! Khi nào có việc sếp cứ gọi em. 🤖",
        "Dạ bye bye sếp! Chúc sếp một ngày tuyệt vời ạ. ✨"
    ];

    private static readonly GOODBYE_EN = [
        "Goodbye! Have a wonderful day ahead. Let me know when you need me again. 😊",
        "Bye! Talk to you later. I'll be right here waiting for your next command. 🫡",
        "Farewell! Take care and talk to you soon. 🤖",
        "Goodbye! Glad I could help you today. ✨"
    ];

    private static getRandomResponse(responses: string[]): string {
        const index = Math.floor(Math.random() * responses.length);
        return responses[index];
    }

    /**
     * Matches a user query against known chitchat intents and returns a response,
     * or null if the query does not match.
     */
    public static matchAndRespond(query: string): string | null {
        const q = query.trim().toLowerCase();
        if (!q || q.length >= 60) return null;

        // 1. Identity Check
        if (
            /(tên gì|tên là gì|bạn là ai|em là ai|là con gì|tên em|tên cậu|who are you|what is your name|your name|identity|who is this|who's this)/i.test(q)
        ) {
            const isEnglish = /(who|what|your|name|identity)/i.test(q);
            return this.getRandomResponse(isEnglish ? this.IDENTITY_EN : this.IDENTITY_VI);
        }

        // 2. Goodbye Check
        if (
            /(tạm biệt|tạm_biệt|hẹn gặp lại|hẹn gặp|gặp lại sau|bye|bye bye|see you|goodbye|farewell)/i.test(q)
        ) {
            const isEnglish = /(bye|see you|goodbye|farewell)/i.test(q);
            return this.getRandomResponse(isEnglish ? this.GOODBYE_EN : this.GOODBYE_VI);
        }

        // 3. Pleasantries / Health Check
        if (
            /(khỏe không|khỏe_không|thế nào rồi|dạo này sao|hôm nay thế nào|how are you|how is it going|how's it going|how are things|doing today)/i.test(q)
        ) {
            const isEnglish = /(how|things|doing today)/i.test(q);
            return this.getRandomResponse(isEnglish ? this.HEALTH_EN : this.HEALTH_VI);
        }

        // 4. Thanks Check
        if (
            /(cảm ơn|cám ơn|thank|thanks|tks|thank you|thank u)/i.test(q)
        ) {
            const isEnglish = /(thank|thanks|tks)/i.test(q);
            return this.getRandomResponse(isEnglish ? this.THANKS_EN : this.THANKS_VI);
        }

        // 5. Greetings Check
        if (
            /^(chào|xin chào|lô sếp|hello|hi\b|hey\b|helo\b|good morning|good afternoon|good evening)/i.test(q) ||
            /^(xin_chào|chào sếp|chào_sếp|chào bạn)/i.test(q)
        ) {
            const isEnglish = /^(hello|hi\b|hey\b|helo\b|good)/i.test(q);
            return this.getRandomResponse(isEnglish ? this.GREETINGS_EN : this.GREETINGS_VI);
        }

        return null;
    }
}
