import { ref, shallowRef, triggerRef, watch, nextTick } from "vue";

export function useChat(
  t: (key: string) => string,
  stopQueuedAudio: () => void,
  sendMsg: (event: string, payload?: any) => void
) {
  const inputText = ref("");
  const isThinking = ref(false);
  const isCollapsed = ref(true);
  const chatContainer = ref<HTMLElement | null>(null);

  const messages = shallowRef<{ role: "user" | "assistant"; text: string; thinking?: string }[]>([
    {
      role: "assistant",
      text: t("welcome_liva"),
    },
  ]);

  let typingDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let lastSentTypingText = "";

  watch(inputText, (newVal) => {
    if (typingDebounceTimer) {
      clearTimeout(typingDebounceTimer);
    }

    const cleanVal = newVal.trim();

    if (cleanVal.length === 0) {
      if (lastSentTypingText !== "") {
        lastSentTypingText = "";
        sendMsg("user_typing_cancelled");
      }
      return;
    }

    if (cleanVal.length >= 5 && cleanVal !== lastSentTypingText) {
      typingDebounceTimer = setTimeout(() => {
        sendMsg("user_typing", { text: cleanVal });
      }, 500);
    }
  });

  const startNewChat = () => {
    messages.value = [
      {
        role: "assistant",
        text: t("welcome_liva"),
      },
    ];
    triggerRef(messages);
    stopQueuedAudio();
    if (isCollapsed.value) {
      isCollapsed.value = false;
    }
  };

  const sendMessage = (ws: WebSocket | null) => {
    if (!inputText.value.trim() || !ws || ws.readyState !== WebSocket.OPEN) return;

    stopQueuedAudio();

    const text = inputText.value.trim();
    messages.value = [...messages.value, { role: "user", text }];
    triggerRef(messages);

    ws.send(
      JSON.stringify({
        event: "user_voice_command",
        payload: { text },
      })
    );

    inputText.value = "";
    scrollToBottom();
  };

  const renderRichText = (text: string) => {
    if (!text) return "";
    let out = text;

    if (out.includes("Zalo") && out.includes("Messenger") && out.includes("Email")) {
      out = out.replace(
        /(<br\/>)?\s*[-*•]\s*💬\s*Zalo/gi,
        '<br/><button class="hitl-btn hitl-btn-approve" style="margin-top:6px; padding: 6px 16px; width: 100%; justify-content: flex-start; text-align: left;" onclick="window.sendLIVAMessage(\'Zalo\')">💬 Zalo</button>'
      );
      out = out.replace(
        /(<br\/>)?\s*[-*•]\s*📘\s*Messenger/gi,
        '<br/><button class="hitl-btn hitl-btn-approve" style="background: linear-gradient(135deg, #1d4ed8 0%, #3b82f6 100%); margin-top:6px; padding: 6px 16px; width: 100%; justify-content: flex-start; text-align: left;" onclick="window.sendLIVAMessage(\'Messenger\')">📘 Messenger</button>'
      );
      out = out.replace(
        /(<br\/>)?\s*[-*•]\s*📧\s*Email/gi,
        '<br/><button class="hitl-btn hitl-btn-approve" style="background: linear-gradient(135deg, #ea580c 0%, #f97316 100%); margin-top:6px; padding: 6px 16px; width: 100%; justify-content: flex-start; text-align: left;" onclick="window.sendLIVAMessage(\'Email\')">📧 Email</button>'
      );

      if (!out.includes("window.sendLIVAMessage")) {
        out += `<div style="margin-top: 12px; display: flex; flex-direction: column; gap: 6px;">
          <button class="hitl-btn hitl-btn-approve" style="padding: 6px 16px; width: 100%; justify-content: flex-start; text-align: left;" onclick="window.sendLIVAMessage('Zalo')">💬 Zalo</button>
          <button class="hitl-btn hitl-btn-approve" style="background: linear-gradient(135deg, #1d4ed8 0%, #3b82f6 100%); padding: 6px 16px; width: 100%; justify-content: flex-start; text-align: left;" onclick="window.sendLIVAMessage('Messenger')">📘 Messenger</button>
          <button class="hitl-btn hitl-btn-approve" style="background: linear-gradient(135deg, #ea580c 0%, #f97316 100%); padding: 6px 16px; width: 100%; justify-content: flex-start; text-align: left;" onclick="window.sendLIVAMessage('Email')">📧 Email</button>
        </div>`;
      }
    }

    return out;
  };

  const scrollToBottom = async () => {
    await nextTick();
    if (chatContainer.value) {
      chatContainer.value.scrollTop = chatContainer.value.scrollHeight;
    }
  };

  const cleanup = () => {
    if (typingDebounceTimer) {
      clearTimeout(typingDebounceTimer);
      typingDebounceTimer = null;
    }
  };

  return {
    inputText,
    isThinking,
    isCollapsed,
    messages,
    chatContainer,
    startNewChat,
    sendMessage,
    renderRichText,
    scrollToBottom,
    cleanup,
  };
}
