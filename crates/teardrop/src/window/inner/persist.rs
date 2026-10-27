use super::*;
// ----------------------------------------------------------- persistence

pub(super) fn save_all() {
    let (data, _cache) = paths::ensure_dirs();
    SETTINGS.with(|s| {
        VIEW.with(|v| {
            let vm = v.borrow();
            let mut st = s.borrow_mut();
            st.goal_ml = vm.water.goal_ml.max(500);
            st.idle_look = vm.idle_look;
            st.start_with_windows = startup::enabled();
            store::save(&data, "settings", &*st);
            store::save(&data, "page", &vm.page);
            store::save(&data, "water", &vm.water);
            store::save(&data, "tasks", &vm.tasks);
            store::save(&data, "notes", &vm.notes);
            store::save(&data, "shelf", &vm.shelf);
        });
    });
}

pub(super) fn load_all() {
    let (data, _cache) = paths::ensure_dirs();
    SETTINGS.with(|s| {
        let mut st = s.borrow_mut();
        *st = store::load(&data, "settings");
        st.start_with_windows = startup::enabled();
    });
    // Refresh the active provider's catalog in the background so the model
    // picker is never stuck on the curated fallback after a launch.
    let (provider, url) = SETTINGS.with(|s| {
        let st = s.borrow();
        (st.ai_provider, st.active_ai_url().to_string())
    });
    crate::ai::refresh_models(provider, url);
    VIEW.with(|v| {
        let mut vm = v.borrow_mut();
        let page: Page = store::load(&data, "page");
        vm.page = if page == Page::Settings { Page::Music } else { page };
        vm.water = store::load(&data, "water");
        // First reminder comes one interval after launch, not instantly.
        vm.water.last_reminder = Some(Utc::now());
        vm.tasks = store::load(&data, "tasks");
        vm.notes = store::load(&data, "notes");
        vm.shelf = store::load(&data, "shelf");
        SETTINGS.with(|s| {
            let st = s.borrow();
            vm.water.goal_ml = st.goal_ml.max(500);
            vm.water.reminders_enabled = st.water_reminders;
            vm.idle_look = st.idle_look;
        });
        CPU.with(|c| vm.system = sample_all(&mut c.borrow_mut(), &vm.system));
        // A page hidden after it was last saved stays hidden on restore.
        let vis = SETTINGS.with(|s| crate::ui::layout::visible_pages(&s.borrow()));
        if !vis.contains(&vm.page) {
            vm.page = vis[0];
        }
        UI.with(|u| *u.borrow_mut() = UiState::new(vm.page));
    });
    AI_REV.set(u32::MAX);
    sync_ai();
}
