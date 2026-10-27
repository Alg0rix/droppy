use super::*;
// ----------------------------------------- Draining & Undoing App Mutations

/// Drain any newly parsed `[ACTION:...]` directives so `window.rs` can apply them
/// to `AppState` and record their `AiActionBadge`s.
pub fn take_pending_directives() -> Vec<(u64, usize, Vec<ParsedDirective>)> {
    with_runtime(|rt| std::mem::take(&mut rt.pending_directives))
}

/// Attach executed `AiActionBadge`s to a message and persist `chats.json`.
pub fn record_action_badges(conv_id: u64, msg_idx: usize, badges: Vec<AiActionBadge>, new_memories: Vec<String>, provider: AiProvider) {
    with_runtime(|rt| {
        for mem in new_memories {
            if !rt.store.memories.iter().any(|m| m.eq_ignore_ascii_case(&mem)) {
                rt.store.memories.push(mem);
                if rt.store.memories.len() > MAX_MEMORIES {
                    rt.store.memories.remove(0);
                }
            }
        }
        if let Some(conv) = rt.store.conversations.iter_mut().find(|c| c.id == conv_id) {
            if let Some(msg) = conv.messages.get_mut(msg_idx) {
                msg.actions.extend(badges);
            }
        }
        save_chat_store(&rt.store);
        rt.rebuild_view(provider);
    });
}

/// Mark an action badge on the active conversation as undone and return its `UndoOp`.
pub fn mark_action_undone(msg_idx: usize, action_idx: usize, provider: AiProvider) -> Option<UndoOp> {
    with_runtime(|rt| {
        let active_id = rt.store.active_id;
        let mut op = None;
        if let Some(conv) = rt.store.conversations.iter_mut().find(|c| c.id == active_id) {
            if let Some(msg) = conv.messages.get_mut(msg_idx) {
                if let Some(act) = msg.actions.get_mut(action_idx) {
                    if act.can_undo() {
                        act.undone = true;
                        op = Some(act.undo.clone());
                    }
                }
            }
        }
        if let Some(UndoOp::ForgetMemory(ref fact)) = op {
            rt.store.memories.retain(|m| m != fact);
        }
        if op.is_some() {
            save_chat_store(&rt.store);
            rt.rebuild_view(provider);
        }
        op
    })
}
