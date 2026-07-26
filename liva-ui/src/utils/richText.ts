const CHANNELS = [
  { name: "Zalo", label: "💬 Zalo" },
  { name: "Messenger", label: "📘 Messenger" },
  { name: "Email", label: "📧 Email" },
] as const;

export type RichTextChannel = (typeof CHANNELS)[number]["name"];

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function channelButton(channel: RichTextChannel, label: string): string {
  return `<button type="button" class="hitl-btn hitl-btn-approve hitl-channel-btn" data-liva-channel="${channel}">${label}</button>`;
}

/**
 * Render untrusted model/gateway text for `v-html`.
 *
 * All input is escaped before the only allowed markup (`br` and fixed channel
 * buttons) is generated. No input attribute, URL, tag, or inline script can
 * survive into the returned HTML.
 */
export function renderSafeRichText(text: string): string {
  if (!text) return "";

  const normalized = text.replace(/<br\s*\/?>/gi, "\n");
  let rendered = escapeHtml(normalized);
  const containsAllChannels = CHANNELS.every(channel =>
    normalized.toLocaleLowerCase().includes(channel.name.toLocaleLowerCase())
  );

  if (containsAllChannels) {
    let replacedListItem = false;
    for (const channel of CHANNELS) {
      const icon = channel.label.split(" ")[0];
      const pattern = new RegExp(
        `(^|\\n)\\s*[-*•]\\s*${icon}\\s*${channel.name}`,
        "gi",
      );
      rendered = rendered.replace(pattern, (_match, prefix: string) => {
        replacedListItem = true;
        return `${prefix}${channelButton(channel.name, channel.label)}`;
      });
    }

    if (!replacedListItem) {
      rendered += `\n<div class="hitl-container hitl-channel-container">${CHANNELS
        .map(channel => channelButton(channel.name, channel.label))
        .join("")}</div>`;
    }
  }

  return rendered.replaceAll("\n", "<br/>");
}

export function readRichTextChannel(target: EventTarget | null): RichTextChannel | null {
  if (!(target instanceof Element)) return null;
  const button = target.closest<HTMLElement>("[data-liva-channel]");
  const value = button?.dataset.livaChannel;
  return CHANNELS.some(channel => channel.name === value)
    ? value as RichTextChannel
    : null;
}
