use std::{
    collections::{HashMap, HashSet},
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::Arc,
};

use dioxus::prelude::{
    use_context, use_context_provider, Coroutine, Ref, RefCell, RefMut, ScopeId, ScopeState,
};

use crate::{
    lsp::{create_lsp, LSPBridge, LspConfig},
    use_editable::EditorData,
};

#[derive(Clone)]
pub enum PanelTab {
    TextEditor(EditorData),
    Config,
}

impl PanelTab {
    pub fn get_data(&self) -> (String, String) {
        match self {
            PanelTab::Config => ("config".to_string(), "Config".to_string()),
            PanelTab::TextEditor(editor) => (
                editor.path().to_str().unwrap().to_owned(),
                editor
                    .path()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
            ),
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

pub fn use_init_manager<'a>(
    cx: &'a ScopeState,
    lsp_status_coroutine: &'a Coroutine<(String, String)>,
) -> &'a EditorManagerInnerWrapper {
    use_context_provider(cx, || {
        EditorManagerInnerWrapper::new(cx, EditorManager::new(lsp_status_coroutine.clone()))
    })
}

pub fn use_manager(cx: &ScopeState) -> EditorManagerWrapper {
    let mut manager = use_context::<EditorManagerInnerWrapper>(cx)
        .unwrap()
        .clone();

    manager.scope = cx.scope_id();

    EditorManagerWrapper::new(cx, manager)
}

#[derive(Clone)]
pub struct EditorManagerInnerWrapper {
    pub subscribers: Rc<RefCell<HashSet<ScopeId>>>,
    value: Rc<RefCell<EditorManager>>,
    scheduler: Arc<dyn Fn(ScopeId) + Send + Sync>,
    scope: ScopeId,
}

impl PartialEq for EditorManagerInnerWrapper {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct EditorManagerWrapper {
    inner: EditorManagerInnerWrapper,
}

impl Drop for EditorManagerWrapper {
    fn drop(&mut self) {
        self.inner
            .subscribers
            .borrow_mut()
            .remove(&self.inner.scope);
    }
}

impl EditorManagerWrapper {
    pub fn new(cx: &ScopeState, inner: EditorManagerInnerWrapper) -> Self {
        inner.subscribers.borrow_mut().insert(cx.scope_id());
        Self { inner }
    }

    pub fn global_write(&self) -> EditorManagerWrapperGuard {
        self.inner.global_write()
    }

    pub fn write(&self) -> EditorManagerWrapperGuard {
        self.inner.write()
    }

    pub fn current(&self) -> Ref<EditorManager> {
        self.inner.current()
    }
}

pub struct EditorManagerWrapperGuard<'a> {
    pub subscribers: Rc<RefCell<HashSet<ScopeId>>>,
    value: RefMut<'a, EditorManager>,
    scheduler: Arc<dyn Fn(ScopeId) + Send + Sync>,
    scope: Option<ScopeId>,
}

impl EditorManagerInnerWrapper {
    pub fn new(cx: &ScopeState, value: EditorManager) -> Self {
        Self {
            subscribers: Rc::new(RefCell::new(HashSet::from([cx.scope_id()]))),
            value: Rc::new(RefCell::new(value.clone())),
            scheduler: cx.schedule_update_any(),
            scope: cx.scope_id(),
        }
    }

    pub fn global_write(&self) -> EditorManagerWrapperGuard {
        EditorManagerWrapperGuard {
            subscribers: self.subscribers.clone(),
            value: self.value.borrow_mut(),
            scheduler: self.scheduler.clone(),
            scope: None,
        }
    }

    pub fn write(&self) -> EditorManagerWrapperGuard {
        EditorManagerWrapperGuard {
            subscribers: self.subscribers.clone(),
            value: self.value.borrow_mut(),
            scheduler: self.scheduler.clone(),
            scope: Some(self.scope),
        }
    }

    pub fn current(&self) -> Ref<EditorManager> {
        self.value.borrow()
    }
}

impl Drop for EditorManagerWrapperGuard<'_> {
    fn drop(&mut self) {
        if let Some(id) = self.scope {
            (self.scheduler)(id)
        } else {
            for scope_id in self.subscribers.borrow().iter() {
                (self.scheduler)(*scope_id)
            }
        }
    }
}

impl<'a> Deref for EditorManagerWrapperGuard<'a> {
    type Target = RefMut<'a, EditorManager>;

    fn deref(&self) -> &RefMut<'a, EditorManager> {
        &self.value
    }
}

impl<'a> DerefMut for EditorManagerWrapperGuard<'a> {
    fn deref_mut(&mut self) -> &mut RefMut<'a, EditorManager> {
        &mut self.value
    }
}

#[derive(Clone)]
pub struct EditorManager {
    pub is_focused: bool,
    pub focused_panel: usize,
    pub panes: Vec<Panel>,
    pub font_size: f32,
    pub line_height: f32,
    pub language_servers: HashMap<String, LSPBridge>,
    pub lsp_status_coroutine: Coroutine<(String, String)>,
}

impl EditorManager {
    pub fn new(lsp_status_coroutine: Coroutine<(String, String)>) -> Self {
        Self {
            is_focused: true,
            focused_panel: 0,
            panes: vec![Panel::new()],
            font_size: 17.0,
            line_height: 1.2,
            language_servers: HashMap::default(),
            lsp_status_coroutine,
        }
    }

    pub fn set_fontsize(&mut self, fontsize: f32) {
        self.font_size = fontsize;
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.is_focused = focused;
    }

    pub fn is_focused(&self) -> bool {
        self.is_focused
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
        let opened_tab = self.panes[panel]
            .tabs
            .iter()
            .enumerate()
            .find(|(_, t)| t.get_data().0 == tab.get_data().0);

        if let Some((tab_index, _)) = opened_tab {
            if focus {
                self.focused_panel = panel;
                self.panes[panel].active_tab = Some(tab_index);
            }
        } else {
            self.panes[panel].tabs.push(tab);

            if focus {
                self.focused_panel = panel;
                self.panes[panel].active_tab = Some(self.panes[panel].tabs.len() - 1);
            }
        }
    }

    pub fn close_editor(&mut self, panel: usize, editor: usize) {
        if let Some(active_tab) = self.panes[panel].active_tab {
            let prev_editor = editor > 0;
            let next_editor = self.panes[panel].tabs.get(editor + 1).is_some();
            if active_tab == editor {
                self.panes[panel].active_tab = if next_editor {
                    Some(editor)
                } else if prev_editor {
                    Some(editor - 1)
                } else {
                    None
                };
            } else if active_tab >= editor {
                self.panes[panel].active_tab = Some(active_tab - 1);
            }
        }

        self.panes[panel].tabs.remove(editor);
    }

    pub fn push_panel(&mut self, panel: Panel) {
        self.panes.push(panel);
    }

    pub fn panels(&self) -> &[Panel] {
        &self.panes
    }

    pub fn panel(&self, panel: usize) -> &Panel {
        &self.panes[panel]
    }

    pub fn panel_mut(&mut self, panel: usize) -> &mut Panel {
        &mut self.panes[panel]
    }

    pub fn set_focused_panel(&mut self, panel: usize) {
        self.focused_panel = panel;
    }

    pub fn close_panel(&mut self, panel: usize) {
        if self.panes.len() > 1 {
            self.panes.remove(panel);
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

    pub async fn get_or_insert_lsp(
        manager: EditorManagerWrapper,
        lsp_config: &LspConfig,
    ) -> LSPBridge {
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