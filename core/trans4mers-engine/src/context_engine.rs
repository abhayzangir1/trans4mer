use trans4mers_domain::execution::ExecutionState;
use trans4mers_domain::learning::LearnedRule;
use trans4mers_domain::memory::Memory;
use trans4mers_domain::provider::LlmMessage;

pub struct ContextEngine;

impl ContextEngine {
    pub fn estimate_tokens(text: &str) -> usize {
        text.len() / 4
    }

    /// Assembles the complete prompt array for the LLM.
    /// Combines the strict system prompt, learned rules, episodic memory, and the conversational history.
    pub fn assemble_prompt(
        state: &ExecutionState,
        global_instructions: &str,
        learned_rules: &[LearnedRule],
        episodic_memories: &[Memory],
    ) -> Vec<LlmMessage> {
        Self::assemble_prompt_with_context(
            state,
            global_instructions,
            learned_rules,
            episodic_memories,
            None,
            None,
            None,
            Some(8192),
        )
    }

    /// Assembles the prompt with dynamic tool manifest injection and system status metadata.
    #[allow(clippy::too_many_arguments)]
    pub fn assemble_prompt_with_context(
        state: &ExecutionState,
        global_instructions: &str,
        learned_rules: &[LearnedRule],
        episodic_memories: &[Memory],
        available_tools: Option<&[String]>,
        system_status: Option<&str>,
        available_skills: Option<&[&crate::skill_loader::SkillDefinition]>,
        max_context_tokens: Option<usize>,
    ) -> Vec<LlmMessage> {
        let max_tokens = max_context_tokens.unwrap_or(8192);
        let mut messages = Vec::new();
        let mut current_tokens = 0;

        // 1. Core System Prompt & Role Contract
        let mut system_content = format!(
            "You are an autonomous AI Agent in the trans4mers architecture.\n\
             ROLE CONTRACT & GLOBAL INSTRUCTIONS:\n{}\n\n",
            global_instructions
        );

        // 2. Dynamic Tool Manifest & Escalation Policy Injection
        system_content.push_str(
            "CAPABILITY & ESCALATION POLICY:\n\
             - You have access to local workspace tools (Filesystem, Terminal, Browser, Documents, Scheduler, Memory).\n\
             - If an operation requires elevated capability or risks modifying critical files, DO NOT abort or claim lack of permissions.\n\
             - Always invoke the appropriate tool directly. The trans4mers policy engine will intercept sensitive actions and present an interactive Human Approval Card to the user.\n\
             - For continuous background jobs or timed automations, use the `scheduler.create_schedule` tool.\n\n"
        );

        if let Some(tools) = available_tools
            && !tools.is_empty()
        {
            system_content.push_str("AVAILABLE CAPABILITIES & TOOLS:\n");
            for t in tools {
                system_content.push_str(&format!("- `{}`\n", t));
            }
            system_content.push('\n');
        }

        if let Some(skills) = available_skills
            && !skills.is_empty()
        {
            system_content.push_str("LOADED WORKFLOW SKILLS & PROCEDURES:\n");
            for s in skills {
                system_content.push_str(&format!(
                    "- **{}** (v{}): {}\n",
                    s.name, s.version, s.description
                ));
                for step in &s.steps {
                    system_content.push_str(&format!(
                        "    * Step `{}`: {}\n",
                        step.name, step.instruction
                    ));
                }
            }
            system_content.push('\n');
        }

        // 3. System Status
        if let Some(status) = system_status {
            system_content.push_str(&format!("NODE RUNTIME STATUS:\n{}\n\n", status));
        }

        // 6. Interaction & Tool Execution Guidelines (moved up)
        system_content.push_str(
            "INTERACTION & TOOL EXECUTION GUIDELINES:\n\
             - When answering questions, planning, or addressing the user, reply directly in clear, helpful natural language markdown.\n\
             - Only invoke a tool when you genuinely need to read/write workspace files, execute shell commands, manage schedules, or delegate work.\n\
             - Once a tool execution finishes and you receive its Result, ANALYZE the result and reply to the user in natural language summarizing what you found or accomplished. Do NOT invoke the same tool again with identical arguments.\n\
             - If you have completed the requested task or answered the user's question, reply directly to the user with your final response. You do not need to call any more tools.\n\
             - Never output raw JSON when you intend to talk to the user. Speak in clear English markdown.\n\
             - Never invent non-existent tool names. If no tool is needed, simply provide your response.\n\
             - If human code edits or comments are received, integrate them faithfully into your next step.\n"
        );

        let system_tokens = Self::estimate_tokens(&system_content);
        current_tokens += system_tokens;

        messages.push(LlmMessage {
            role: "system".to_string(),
            content: system_content,
        });

        // 7. Inject execution history (Recent Steps)
        // Add them to a temporary list so we can count them
        let mut history_messages = Vec::new();
        for step in &state.steps {
            if step.action_intent.is_none()
                && (step.thought.starts_with("User Instruction from")
                    || step.thought.starts_with("I received a new message"))
            {
                history_messages.push(LlmMessage {
                    role: "user".to_string(),
                    content: step.thought.clone(),
                });
                continue;
            }

            let action_json_str = match &step.action_intent {
                Some(act) => serde_json::to_string(act).unwrap_or_else(|_| "null".to_string()),
                None => "null".to_string(),
            };

            history_messages.push(LlmMessage {
                role: "assistant".to_string(),
                content: format!(
                    "Thought: {}\nAction: {}",
                    step.thought, action_json_str
                ),
            });

            let result_str = if let Some(err) = &step.error {
                format!("ERROR: {}: {}", err.error_type, err.message)
            } else if let Some(payload) = &step.result_payload {
                match payload {
                    serde_json::Value::String(s) => {
                        if s.trim().is_empty() {
                            "Success".to_string()
                        } else {
                            s.clone()
                        }
                    }
                    serde_json::Value::Null => "Success".to_string(),
                    other => other.to_string(),
                }
            } else {
                "Success".to_string()
            };

            history_messages.push(LlmMessage {
                role: "user".to_string(),
                content: format!("Result: {}", result_str),
            });
        }

        // Add history messages from latest to oldest until budget is tight, but we typically want to preserve history order.
        // Actually, let's keep all recent steps if possible, or just truncate oldest.
        // We'll iterate backwards and collect, then reverse.
        let mut final_history = Vec::new();
        for msg in history_messages.into_iter().rev() {
            let tokens = Self::estimate_tokens(&msg.content);
            if current_tokens + tokens > max_tokens {
                break;
            }
            current_tokens += tokens;
            final_history.push(msg);
        }
        final_history.reverse();

        let mut memories_content = String::new();
        if !episodic_memories.is_empty() {
            memories_content.push_str("RELEVANT MEMORIES (Context from past events):\n");
            for memory in episodic_memories {
                let mem_str = format!("- [{}]\n", memory.content);
                let tokens = Self::estimate_tokens(&mem_str);
                if current_tokens + tokens < max_tokens {
                    memories_content.push_str(&mem_str);
                    current_tokens += tokens;
                }
            }
            memories_content.push('\n');
        }

        let mut rules_content = String::new();
        if !learned_rules.is_empty() {
            rules_content.push_str("LEARNED RULES (Strict compliance required):\n");
            for rule in learned_rules {
                let rule_str = format!("- [{} Confidence] {}\n", rule.confidence, rule.rule_text);
                let tokens = Self::estimate_tokens(&rule_str);
                if current_tokens + tokens < max_tokens {
                    rules_content.push_str(&rule_str);
                    current_tokens += tokens;
                }
            }
            rules_content.push('\n');
        }

        // We can inject rules and memories by either appending to the first system message or inserting them as a new system message.
        // Since we already pushed the first one, let's just append to it if possible, or push a new one.
        if !memories_content.trim().is_empty() || !rules_content.trim().is_empty() {
            messages[0]
                .content
                .push_str(&format!("\n{}{}", memories_content, rules_content));
        }

        messages.extend(final_history);
        messages
    }
}
