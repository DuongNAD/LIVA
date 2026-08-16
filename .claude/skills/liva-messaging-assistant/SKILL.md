---
name: liva-messaging-assistant
description: Manage contacts and safely draft, preview, and send messages across Telegram and Messenger using LIVA's two-phase confirmation protocol. Use when drafting messages, resolving contacts, looking up pending outbox items, or sending system alerts.
---

# LIVA Messaging Assistant

## Workflow

1. **Resolve Contact & Platform**: Look up recipient in LIVA's local contact store via `contacts:list`. Use fuzzy normalized matching for Vietnamese names (removing diacritics/accents via `normalize_for_match`).
2. **Handle Ambiguity**: If multiple contacts match (`Resolution::Ambiguous`), prompt the user to choose the exact recipient and platform (`telegram` or `messenger`). Never guess or send to an unconfirmed recipient.
3. **Draft Message (`message:draft`)**: Create a pending draft record containing recipient details, platform, and text payload. Return the generated `draftId` and preview card for user verification.
4. **Enforce Two-Phase Safety Gate**:
   - **NEVER** bypass `message:draft` to fire raw external messages.
   - Wait for explicit user confirmation before issuing `message:confirm` with the corresponding `draftId`.
   - If the user rejects or cancels, invoke `message:cancel` with the `draftId`.
5. **Manage Contacts (`contacts:upsert` / `contacts:delete`)**:
   - Add or update contact details with valid platform-specific handles (validate Telegram chat IDs and Messenger handles before saving).
6. **System Notifications (`telegram:send_text`)**: Use direct system notifications only for non-interactive scheduled background digests or proactive alerts, never for user-directed personal messaging.

## Platform Constraints

- **Telegram**: Requires valid numeric `chatId` or configured `TELEGRAM_BOT_TOKEN`.
- **Messenger**: Requires active Messenger sidecar/connector status (`messenger:status`).

## Stop Conditions

Stop and report when:
- Contact lookup yields zero matches and no recipient handle is provided.
- Recipient resolution is ambiguous without user selection.
- User explicitly declines or cancels the message draft.
- Required messaging platform token or connector is offline/unconfigured.
