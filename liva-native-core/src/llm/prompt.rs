use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

pub fn compile_gemma_prompt(messages: &[ChatMessage]) -> Result<String, String> {
    if messages.is_empty() {
        return Err("Cannot compile empty chat completions message array".to_string());
    }

    let mut merged = Vec::new();
    let mut system_instructions = String::new();

    for msg in messages {
        if msg.role == "system" {
            if !system_instructions.is_empty() {
                system_instructions.push('\n');
            }
            system_instructions.push_str(&msg.content);
        } else {
            merged.push(msg.clone());
        }
    }

    if merged.is_empty() && !system_instructions.is_empty() {
        // If there is only system instructions, turn it into a user message
        merged.push(ChatMessage {
            role: "user".to_string(),
            content: "".to_string(),
        });
    } else if merged.is_empty() {
        return Err(
            "Cannot compile chat completions without user or assistant messages".to_string(),
        );
    }

    let mut prompt_text = String::new();
    for (idx, msg) in merged.iter().enumerate() {
        let role = if msg.role == "assistant" {
            "model"
        } else {
            &msg.role
        };
        let content = if idx == 0 && !system_instructions.is_empty() {
            if msg.content.is_empty() {
                system_instructions.clone()
            } else {
                format!("{}\n\n{}", system_instructions, msg.content)
            }
        } else {
            msg.content.clone()
        };
        prompt_text.push_str(&format!(
            "<start_of_turn>{}\n{}<end_of_turn>\n",
            role, content
        ));
    }
    prompt_text.push_str("<start_of_turn>model\n");

    Ok(prompt_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_gemma_prompt_empty() {
        let messages = vec![];
        let result = compile_gemma_prompt(&messages);
        assert!(result.is_err());
    }

    #[test]
    fn test_compile_gemma_prompt_system_only() {
        let messages = vec![ChatMessage {
            role: "system".to_string(),
            content: "You are a helpful assistant.".to_string(),
        }];
        let result = compile_gemma_prompt(&messages).unwrap();
        assert_eq!(
            result,
            "<start_of_turn>user\nYou are a helpful assistant.<end_of_turn>\n<start_of_turn>model\n"
        );
    }

    #[test]
    fn test_compile_gemma_prompt_standard() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: "Be concise.".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Hello!".to_string(),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "Hi!".to_string(),
            },
        ];
        let result = compile_gemma_prompt(&messages).unwrap();
        assert_eq!(
            result,
            "<start_of_turn>user\nBe concise.\n\nHello!<end_of_turn>\n<start_of_turn>model\nHi!<end_of_turn>\n<start_of_turn>model\n"
        );
    }
}
