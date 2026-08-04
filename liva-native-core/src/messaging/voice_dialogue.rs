use super::contacts::Platform;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceMessageAction {
    AskPlatform,
    AskBody,
    Draft {
        recipient: String,
        body: String,
        platform: Platform,
    },
    Confirm {
        draft_id: String,
    },
    Cancel {
        draft_id: String,
    },
    RepeatConfirmation,
}

#[derive(Debug, Clone)]
enum PendingVoiceMessage {
    Body {
        recipient: String,
        platform: Option<Platform>,
    },
    Platform {
        recipient: String,
        body: String,
    },
    Confirmation {
        draft_id: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct VoiceMessageDialogue {
    pending: Option<PendingVoiceMessage>,
}

impl VoiceMessageDialogue {
    pub fn begin(
        &mut self,
        recipient: String,
        body: String,
        platform: Option<Platform>,
    ) -> VoiceMessageAction {
        if body.trim().is_empty() {
            self.pending = Some(PendingVoiceMessage::Body {
                recipient,
                platform,
            });
            return VoiceMessageAction::AskBody;
        }

        match platform {
            Some(platform) => VoiceMessageAction::Draft {
                recipient,
                body,
                platform,
            },
            None => {
                self.pending = Some(PendingVoiceMessage::Platform { recipient, body });
                VoiceMessageAction::AskPlatform
            }
        }
    }

    pub fn await_confirmation(&mut self, draft_id: String) {
        self.pending = Some(PendingVoiceMessage::Confirmation { draft_id });
    }

    pub fn clear(&mut self) {
        self.pending = None;
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn follow_up(&mut self, text: &str) -> Option<VoiceMessageAction> {
        match self.pending.as_ref()? {
            PendingVoiceMessage::Body { .. } => {
                let body = text.trim();
                if body.is_empty() {
                    return Some(VoiceMessageAction::AskBody);
                }
                let PendingVoiceMessage::Body {
                    recipient,
                    platform,
                } = self.pending.take()?
                else {
                    unreachable!("pending state was checked above")
                };
                match platform {
                    Some(platform) => Some(VoiceMessageAction::Draft {
                        recipient,
                        body: body.to_string(),
                        platform,
                    }),
                    None => {
                        self.pending = Some(PendingVoiceMessage::Platform {
                            recipient,
                            body: body.to_string(),
                        });
                        Some(VoiceMessageAction::AskPlatform)
                    }
                }
            }
            PendingVoiceMessage::Platform { .. } => {
                let Some(platform) = platform_from_phrase(text) else {
                    return Some(VoiceMessageAction::AskPlatform);
                };
                let PendingVoiceMessage::Platform { recipient, body } = self.pending.take()? else {
                    unreachable!("pending state was checked above")
                };
                Some(VoiceMessageAction::Draft {
                    recipient,
                    body,
                    platform,
                })
            }
            PendingVoiceMessage::Confirmation { .. } if is_confirmation(text) => {
                let PendingVoiceMessage::Confirmation { draft_id } = self.pending.take()? else {
                    unreachable!("pending state was checked above")
                };
                Some(VoiceMessageAction::Confirm { draft_id })
            }
            PendingVoiceMessage::Confirmation { .. } if is_cancellation(text) => {
                let PendingVoiceMessage::Confirmation { draft_id } = self.pending.take()? else {
                    unreachable!("pending state was checked above")
                };
                Some(VoiceMessageAction::Cancel { draft_id })
            }
            PendingVoiceMessage::Confirmation { .. } => {
                Some(VoiceMessageAction::RepeatConfirmation)
            }
        }
    }
}

fn is_cancellation(text: &str) -> bool {
    let normalized = crate::wake::normalize_for_match(text);
    let words: Vec<_> = normalized.split_whitespace().collect();
    words.contains(&"huy")
        || normalized.contains("khong gui")
        || normalized.contains("dung gui")
        || matches!(normalized.as_str(), "thoi" | "thoi khoi")
}

fn is_confirmation(text: &str) -> bool {
    if is_cancellation(text) {
        return false;
    }
    let normalized = crate::wake::normalize_for_match(text);
    matches!(
        normalized.as_str(),
        "gui" | "gui di" | "dong y" | "dong y gui di" | "xac nhan" | "ok" | "oke"
    )
}

fn platform_from_phrase(text: &str) -> Option<Platform> {
    let normalized = crate::wake::normalize_for_match(text);
    normalized.split_whitespace().find_map(|word| {
        match word.trim_matches(|c: char| !c.is_alphanumeric()) {
            "telegram" => Some(Platform::Telegram),
            "messenger" | "messager" | "facebook" | "fb" | "mess" => Some(Platform::Messenger),
            _ => None,
        }
    })
}
