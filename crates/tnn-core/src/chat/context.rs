//! Context-window construction.
//!
//! Prompt processing is expensive on non-AVX2 CPUs, so the builder is
//! deliberately conservative: a short system prompt, recent complete turns
//! only, and an explicit reserve for the model's answer.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::{AppError, Result};
use crate::storage::{Message, MessageRole};

/// Roughly four Latin-1 characters per token; non-ASCII text is denser.
pub fn estimate_tokens(text: &str) -> u32 {
    let mut ascii = 0_u32;
    let mut other = 0_u32;
    for ch in text.chars() {
        if ch.is_ascii() {
            ascii += 1;
        } else {
            other += 1;
        }
    }
    let estimate = ascii.div_ceil(4) + other.div_ceil(2);
    estimate.max(1)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    fn tokens(&self) -> u32 {
        estimate_tokens(&self.content).saturating_add(4)
    }
}

/// Build the message list sent to the engine.
///
/// `context_size` is the server's `--ctx-size`; `max_tokens` is the answer
/// budget. 128 tokens are held back for template overhead and safety.
pub fn build_context(
    system_prompt: &str,
    history: &[Message],
    context_size: u32,
    max_tokens: u32,
) -> Result<Vec<ChatMessage>> {
    let reserve = max_tokens.saturating_add(128);
    let mut budget = context_size.saturating_sub(reserve);
    if budget < 64 {
        return Err(AppError::ContextOverflow);
    }

    let mut messages = Vec::new();
    let system = ChatMessage {
        role: "system".to_string(),
        content: system_prompt.trim().to_string(),
    };
    if !system.content.is_empty() {
        let cost = system.tokens();
        if cost >= budget {
            return Err(AppError::ContextOverflow);
        }
        budget -= cost;
        messages.push(system);
    }

    let mut selected = Vec::new();
    for message in history.iter().rev() {
        if !matches!(
            message.status,
            crate::storage::MessageStatus::Complete | crate::storage::MessageStatus::Truncated
        ) {
            continue;
        }
        let role = match message.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => continue,
        };
        let candidate = ChatMessage {
            role: role.to_string(),
            content: message.content.clone(),
        };
        let cost = candidate.tokens();
        if cost > budget {
            if selected.is_empty() && message.role == MessageRole::User {
                return Err(AppError::ContextOverflow);
            }
            break;
        }
        budget -= cost;
        selected.push(candidate);
    }

    if selected.is_empty() {
        return Err(AppError::ContextOverflow);
    }
    selected.reverse();
    messages.extend(selected);
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{MessageStatus, now_millis};

    #[test]
    fn unfinished_answers_remain_in_continuation_context() {
        let mut answer = message(MessageRole::Assistant, "The unfinished first half");
        answer.status = MessageStatus::Truncated;
        let history = vec![
            message(MessageRole::User, "Explain this"),
            answer,
            message(MessageRole::User, "Continue"),
        ];
        let context = build_context("Be helpful", &history, 2048, 256).unwrap();
        assert!(
            context
                .iter()
                .any(|m| m.role == "assistant" && m.content == "The unfinished first half")
        );
    }

    fn message(role: MessageRole, content: &str) -> Message {
        Message {
            id: "id".into(),
            conversation_id: "c".into(),
            role,
            content: content.into(),
            reasoning: None,
            status: MessageStatus::Complete,
            token_count: None,
            created_at: now_millis(),
            seq: 1,
        }
    }

    #[test]
    fn estimates_reasonably() {
        assert_eq!(estimate_tokens(""), 1);
        assert_eq!(estimate_tokens("hello world"), 3);
        assert_eq!(estimate_tokens("こんにちは世界"), 4);
    }

    #[test]
    fn keeps_recent_history_within_budget() {
        let history = vec![
            message(MessageRole::User, &"old ".repeat(400)),
            message(MessageRole::Assistant, "old answer"),
            message(MessageRole::User, "recent question"),
            message(MessageRole::Assistant, "recent answer"),
            message(MessageRole::User, "latest question"),
        ];
        let context = build_context("You are helpful.", &history, 512, 128).unwrap();
        assert_eq!(context.first().unwrap().role, "system");
        assert_eq!(context.last().unwrap().content, "latest question");
        assert_eq!(context.len(), 5);
        assert!(context.iter().all(|message| message.content.len() < 100));
    }

    #[test]
    fn skips_partial_messages() {
        let mut partial = message(MessageRole::Assistant, "half an answer");
        partial.status = MessageStatus::Interrupted;
        let history = vec![message(MessageRole::User, "question"), partial];
        let context = build_context("sys", &history, 512, 128).unwrap();
        assert_eq!(context.len(), 2);
        assert_eq!(context[1].content, "question");
    }

    #[test]
    fn overflows_when_latest_prompt_does_not_fit() {
        let history = vec![message(MessageRole::User, &"x".repeat(10_000))];
        let error = build_context("sys", &history, 512, 128).unwrap_err();
        assert_eq!(error.code(), "context_overflow");
    }

    #[test]
    fn overflows_with_unreasonable_budget() {
        assert!(build_context("sys", &[message(MessageRole::User, "hi")], 64, 128).is_err());
    }
}
