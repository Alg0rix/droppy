use super::*;
// ------------------------------------------------ Action Extraction & Prompt

/// Strip `[ACTION:...]` tags from the assistant's reply and return `(clean_text, directives)`.
pub fn extract_directives(raw: &str) -> (String, Vec<ParsedDirective>) {
    let mut clean = String::with_capacity(raw.len());
    let mut directives = Vec::new();
    let mut rest = raw;

    while let Some(start) = rest.find("[ACTION:") {
        clean.push_str(&rest[..start]);
        let after_start = &rest[start + 8..];
        if let Some(end) = after_start.find(']') {
            let inner = after_start[..end].trim();
            if let Some((kind, arg)) = inner.split_once(':') {
                let kind = kind.trim().to_ascii_uppercase();
                let arg = arg.trim();
                match kind.as_str() {
                    "ADD_TASK" if !arg.is_empty() => directives.push(ParsedDirective::AddTask(arg.to_string())),
                    "DONE_TASK" if !arg.is_empty() => directives.push(ParsedDirective::DoneTask(arg.to_string())),
                    "DEL_TASK" if !arg.is_empty() => directives.push(ParsedDirective::DelTask(arg.to_string())),
                    "WATER" => {
                        directives.push(ParsedDirective::LogWater(water_arg_ml(arg)));
                    }
                    "FOCUS" if !arg.is_empty() => directives.push(ParsedDirective::Focus(arg.to_ascii_lowercase())),
                    "REMEMBER" if !arg.is_empty() => directives.push(ParsedDirective::Remember(arg.to_string())),
                    "NOTE" if !arg.is_empty() => directives.push(ParsedDirective::Note(arg.to_string())),
                    _ => {}
                }
            }
            rest = &after_start[end + 1..];
        } else {
            clean.push_str(&rest[start..]);
            rest = "";
            break;
        }
    }
    clean.push_str(rest);
    let cleaned = clean.lines().map(str::trim_end).collect::<Vec<_>>().join("\n").trim().to_string();
    (cleaned, directives)
}

pub fn build_system_prompt(day_context: Option<&str>, memories: &[String]) -> String {
    let mut prompt = String::from(
        "You are Droppy AI, a concise, friendly desktop companion living inside the Droppy island at the top of the user's Windows screen.\n\
         Keep replies clear, warm, and compact (2-5 sentences or short bullet points) so they fit comfortably in a compact notch card.\n\n\
         When the user asks you to perform an action in Droppy, include one or more action tags at the very end of your response:\n\
         - Add a task: [ACTION:ADD_TASK:Task title]\n\
         - Complete a task: [ACTION:DONE_TASK:Task title]\n\
         - Delete a task: [ACTION:DEL_TASK:Task title]\n\
         - Log water intake (in glasses; 1 glass = 250 ml): [ACTION:WATER:1]\n\
         - Control Pomodoro focus timer: [ACTION:FOCUS:start] or [ACTION:FOCUS:pause] or [ACTION:FOCUS:reset]\n\
         - Save a durable user preference, habit, or personal fact for future chats: [ACTION:REMEMBER:concise fact]\n\
         - Jot something down in the Notes page: [ACTION:NOTE:one-line note, no newlines]\n\
         Only emit [ACTION:...] tags when the user's request calls for an action or shares a lasting preference.",
    );

    if !memories.is_empty() {
        prompt.push_str("\n\nLearned user memories & preferences:\n");
        for m in memories {
            prompt.push_str("- ");
            prompt.push_str(m);
            prompt.push('\n');
        }
    }

    if let Some(ctx) = day_context.filter(|s| !s.trim().is_empty()) {
        prompt.push_str("\n\nCurrent user day context:\n");
        prompt.push_str(ctx);
    }

    prompt
}
