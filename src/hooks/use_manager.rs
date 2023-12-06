use std::{
    collections::HashMap,
    fmt::Display,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::Arc,
};

use dioxus::prelude::{
    use_context, use_context_provider, Coroutine, Ref, RefCell, RefMut, ScopeId, ScopeState,
};

use crate::lsp::{create_lsp, LSPBridge, LspConfig};

use super::use_editable::EditorData;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum SubscriptionModel {
    All,
    Tab {
        panel_index: usize,
        editor_index: usize,
    },
}

impl SubscriptionModel {
    pub fn new_tab(panel: usize, editor: usize) -> Self {
        Self::Tab {
            panel_index: panel,
            editor_index: editor,
        }
    }
}

#[derive(Clone)]
pub enum PanelTab {
    TextEditor(EditorData),
    Config,
}

#[derive(PartialEq, Eq)]
pub struct PanelTabData {
    pub edited: bool,
    pub title: String,
    pub id: String,
}

impl PanelTab {
    pub fn get_data(&self) -> PanelTabData {
        match self {
            PanelTab::Config => PanelTabData {
                id: "config".to_string(),
                title: "Config".to_string(),
                edited: false,
            },
            PanelTab::TextEditor(editor) => PanelTabData {
                id: editor.path().to_str().unwrap().to_owned(),
                title: editor
                    .path()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
                edited: editor.is_edited(),
            },
        }
    }

    pub fn as_text_editor(&self) -> Option<&EditorData> {
        if let PanelTab::TextEditor(editor_data) = self {
            Some(editor_data)
        } else {
            None
        }
    }

    pub fn as_text_editor_mut(&mut self) -> Option<&mut EditorData> {
        if let PanelTab::TextEditor(editor_data) = self {
            Some(editor_data)
        } else {
            None
        }
    }
}

#[derive(Clone, Default)]
pub struct Panel {
    pub active_tab: Option<usize>,
    pub tabs: Vec<PanelTab>,
}

impl Panel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active_tab(&self) -> Option<usize> {
        self.active_tab
    }

    pub fn tab(&self, editor: usize) -> &PanelTab {
        &self.tabs[editor]
    }

    pub fn tab_mut(&mut self, editor: usize) -> &mut PanelTab {
        &mut self.tabs[editor]
    }

    pub fn tabs(&self) -> &[PanelTab] {
        &self.tabs
    }

    pub fn set_active_tab(&mut self, active_tab: usize) {
        self.active_tab = Some(active_tab);
    }
}

pub type SharedEditorManager = Rc<EditorManagerInner>;

pub fn use_init_manager<'a>(
    cx: &'a ScopeState,
    lsp_status_coroutine: &'a Coroutine<(String, String)>,
) -> &'a SharedEditorManager {
    use_context_provider(cx, || {
        Rc::new(EditorManagerInner::new(
            cx,
            EditorManager::new(lsp_status_coroutine.clone()),
        ))
    })
}

pub fn use_manager(cx: &ScopeState, model: SubscriptionModel) -> &UseManager {
    let manager = use_context::<SharedEditorManager>(cx).unwrap();

    let manager = cx.use_hook(|| {
        let mut manager = manager.as_ref().clone();
        manager.scope = cx.scope_id();
        UseManager::new(manager, model.clone())
    });

    manager.update_model_if_necessary(model);

    manager
}

#[derive(Clone)]
pub struct EditorManagerInner {
    pub subscribers: Rc<RefCell<HashMap<ScopeId, SubscriptionModel>>>,
    value: Rc<RefCell<EditorManager>>,
    scheduler: Arc<dyn Fn(ScopeId) + Send + Sync>,
    scope: ScopeId,
}

impl Drop for EditorManagerInner {
    fn drop(&mut self) {
        self.subscribers.borrow_mut().remove(&self.scope);
    }
}

#[derive(Clone)]
pub struct UseManager {
    inner: SharedEditorManager,
}

impl PartialEq for UseManager {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl UseManager {
    pub fn new(inner: EditorManagerInner, model: SubscriptionModel) -> Self {
        inner.subscribers.borrow_mut().insert(inner.scope, model);
        Self {
            inner: Rc::new(inner),
        }
    }

    fn update_model_if_necessary(&self, model: SubscriptionModel) {
        let mut subs = self.inner.subscribers.borrow_mut();
        let entry = subs.get_mut(&self.inner.scope);

        if let Some(entry) = entry {
            if entry != &model {
                *entry = model
            }
        }
    }

    pub fn global_write(&self) -> EditorManagerInnerGuard {
        self.inner.global_write()
    }

    pub fn write(&self) -> EditorManagerInnerGuard {
        self.inner.write()
    }

    pub fn current(&self) -> Ref<EditorManager> {
        self.inner.current()
    }
}

pub struct EditorManagerInnerGuard<'a> {
    model: SubscriptionModel,
    pub subscribers: Rc<RefCell<HashMap<ScopeId, SubscriptionModel>>>,
    value: RefMut<'a, EditorManager>,
    scheduler: Arc<dyn Fn(ScopeId) + Send + Sync>,
}

impl EditorManagerInner {
    pub fn new(cx: &ScopeState, value: EditorManager) -> Self {
        Self {
            subscribers: Rc::new(RefCell::new(HashMap::from([(
                cx.scope_id(),
                SubscriptionModel::All,
            )]))),
            value: Rc::new(RefCell::new(value.clone())),
            scheduler: cx.schedule_update_any(),
            scope: cx.scope_id(),
        }
    }

    pub fn global_write(&self) -> EditorManagerInnerGuard {
        EditorManagerInnerGuard {
            model: SubscriptionModel::All,
            subscribers: self.subscribers.clone(),
            value: self.value.borrow_mut(),
            scheduler: self.scheduler.clone(),
        }
    }

    pub fn write(&self) -> EditorManagerInnerGuard {
        let model = {
            let subscribers = self.subscribers.borrow();
            subscribers.get(&self.scope).unwrap().clone()
        };
        EditorManagerInnerGuard {
            model,
            subscribers: self.subscribers.clone(),
            value: self.value.borrow_mut(),
            scheduler: self.scheduler.clone(),
        }
    }

    pub fn current(&self) -> Ref<EditorManager> {
        self.value.borrow()
    }
}

impl Drop for EditorManagerInnerGuard<'_> {
    fn drop(&mut self) {
        for (scope_id, scope_model) in self.subscribers.borrow().iter() {
            if scope_model == &self.model {
                (self.scheduler)(*scope_id)
            }
        }
    }
}

impl<'a> Deref for EditorManagerInnerGuard<'a> {
    type Target = RefMut<'a, EditorManager>;

    fn deref(&self) -> &RefMut<'a, EditorManager> {
        &self.value
    }
}

impl<'a> DerefMut for EditorManagerInnerGuard<'a> {
    fn deref_mut(&mut self) -> &mut RefMut<'a, EditorManager> {
        &mut self.value
    }
}

#[derive(Clone, Default, PartialEq)]
pub enum EditorView {
    #[default]
    CodeEditor,
    FilesExplorer,
    Commander,
}

impl Display for EditorView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CodeEditor => f.write_str("Code Editor"),
            Self::FilesExplorer => f.write_str("Files Explorer"),
            Self::Commander => f.write_str("Commander"),
        }
    }
}

#[derive(Clone)]
pub struct EditorManager {
    pub previous_focused_view: Option<EditorView>,
    pub focused_view: EditorView,
    pub focused_panel: usize,
    pub panels: Vec<Panel>,
    pub font_size: f32,
    pub line_height: f32,
    pub language_servers: HashMap<String, LSPBridge>,
    pub lsp_status_coroutine: Coroutine<(String, String)>,
}

impl EditorManager {
    pub fn new(lsp_status_coroutine: Coroutine<(String, String)>) -> Self {
        Self {
            previous_focused_view: None,
            focused_view: EditorView::default(),
            focused_panel: 0,
            panels: vec![Panel::new()],
            font_size: 17.0,
            line_height: 1.2,
            language_servers: HashMap::default(),
            lsp_status_coroutine,
        }
    }

    pub fn set_fontsize(&mut self, fontsize: f32) {
        self.font_size = fontsize;
    }

    pub fn set_focused_view(&mut self, focused_view: EditorView) {
        self.previous_focused_view = Some(self.focused_view.clone());

        self.focused_view = focused_view;
    }

    pub fn focused_view(&self) -> &EditorView {
        &self.focused_view
    }

    pub fn set_focused_view_to_previous(&mut self) {
        if let Some(previous_focused_view) = self.previous_focused_view.clone() {
            self.focused_view = previous_focused_view;
            self.previous_focused_view = None;
        }
    }

    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    pub fn focused_panel(&self) -> usize {
        self.focused_panel
    }

    pub fn push_tab(&mut self, tab: PanelTab, panel: usize, focus: bool) {
        let opened_tab = self.panels[panel]
            .tabs
            .iter()
            .enumerate()
            .find(|(_, t)| t.get_data().id == tab.get_data().id);

        if let Some((tab_index, _)) = opened_tab {
            if focus {
                self.focused_panel = panel;
                self.panels[panel].active_tab = Some(tab_index);
            }
        } else {
            self.panels[panel].tabs.push(tab);

            if focus {
                self.focused_panel = panel;
                self.panels[panel].active_tab = Some(self.panels[panel].tabs.len() - 1);
            }
        }
    }

    pub fn close_editor(&mut self, panel: usize, editor: usize) {
        if let Some(active_tab) = self.panels[panel].active_tab {
            let prev_editor = editor > 0;
            let next_editor = self.panels[panel].tabs.get(editor + 1).is_some();
            if active_tab == editor {
                self.panels[panel].active_tab = if next_editor {
                    Some(editor)
                } else if prev_editor {
                    Some(editor - 1)
                } else {
                    None
                };
            } else if active_tab >= editor {
                self.panels[panel].active_tab = Some(active_tab - 1);
            }
        }

        self.panels[panel].tabs.remove(editor);
    }

    pub fn push_panel(&mut self, panel: Panel) {
        self.panels.push(panel);
    }

    pub fn panels(&self) -> &[Panel] {
        &self.panels
    }

    pub fn panel(&self, panel: usize) -> &Panel {
        &self.panels[panel]
    }

    pub fn panel_mut(&mut self, panel: usize) -> &mut Panel {
        &mut self.panels[panel]
    }

    pub fn set_focused_panel(&mut self, panel: usize) {
        self.focused_panel = panel;
    }

    pub fn close_panel(&mut self, panel: usize) {
        if self.panels.len() > 1 {
            self.panels.remove(panel);
            if self.focused_panel > 0 {
                self.focused_panel -= 1;
            }
        }
    }

    pub fn lsp(&self, lsp_config: &LspConfig) -> Option<&LSPBridge> {
        self.language_servers.get(&lsp_config.language_server)
    }

    pub fn insert_lsp(&mut self, language_server: String, server: LSPBridge) {
        self.language_servers.insert(language_server, server);
    }

    pub async fn get_or_insert_lsp(manager: UseManager, lsp_config: &LspConfig) -> LSPBridge {
        let server = manager.current().lsp(lsp_config).cloned();
        match server {
            Some(server) => server,
            None => {
                let server = create_lsp(lsp_config.clone(), &manager.current()).await;
                manager
                    .write()
                    .insert_lsp(lsp_config.language_server.clone(), server.clone());
                server
            }
        }
    }
}
