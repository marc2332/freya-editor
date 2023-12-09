use std::borrow::Cow;
use std::cell;
use std::cmp::Ordering;
use std::sync::Arc;
use std::sync::Mutex;

use freya::prelude::*;
use skia_safe::scalar;
use skia_safe::textlayout::FontCollection;
use skia_safe::textlayout::ParagraphBuilder;
use skia_safe::textlayout::ParagraphStyle;
use skia_safe::textlayout::TextStyle;
use skia_safe::FontMgr;

use crate::hooks::UseManager;
use crate::parser::*;

#[derive(Clone)]
pub struct UseMetrics {
    paragraph_style: ParagraphStyle,
    font_collection: FontCollection,
    metrics: UseRef<(Arc<Mutex<SyntaxBlocks>>, f32)>,
    manager: UseManager,
    pane_index: usize,
    editor_index: usize,
}

impl UseMetrics {
    pub fn new(
        manager: UseManager,
        metrics: UseRef<(Arc<Mutex<SyntaxBlocks>>, f32)>,
        pane_index: usize,
        editor_index: usize,
    ) -> Self {
        let mut font_collection = FontCollection::new();
        font_collection.set_default_font_manager(FontMgr::default(), "Jetbrains Mono");

        let mut paragraph_style = ParagraphStyle::default();
        let mut text_style = TextStyle::default();
        text_style.set_font_size(manager.current().font_size());
        paragraph_style.set_text_style(&text_style);

        Self {
            paragraph_style,
            font_collection,
            metrics,
            manager,
            pane_index,
            editor_index,
        }
    }

    pub fn get(&self) -> cell::Ref<(Arc<Mutex<SyntaxBlocks>>, f32)> {
        self.metrics.read()
    }

    pub fn run_metrics(&self) {
        let mut paragraph_builder =
            ParagraphBuilder::new(&self.paragraph_style, &self.font_collection);

        let mut longest_line: Vec<Cow<str>> = vec![];

        let manager = self.manager.current();

        let editor = manager
            .panel(self.pane_index)
            .tab(self.editor_index)
            .as_text_editor()
            .unwrap();

        for line in editor.lines() {
            let current_longest_width = longest_line.get(0).map(|l| l.len()).unwrap_or_default();

            let line_len = line.len_chars();

            match line_len.cmp(&current_longest_width) {
                Ordering::Greater => {
                    longest_line.clear();
                    longest_line.push(line.text)
                }
                Ordering::Equal => longest_line.push(line.text),
                _ => {}
            }
        }

        for line in longest_line {
            paragraph_builder.add_text(line);
        }

        let mut paragraph = paragraph_builder.build();

        paragraph.layout(scalar::MAX);

        let (syntax_blocks, width) = &mut *self.metrics.write();

        parse(editor.rope(), &mut syntax_blocks.lock().unwrap());
        *width = paragraph.longest_line();
    }
}

pub fn use_metrics<'a>(
    cx: &'a ScopeState,
    manager: &UseManager,
    pane_index: usize,
    editor_index: usize,
) -> &'a UseMetrics {
    let metrics_ref = use_ref::<(Arc<Mutex<SyntaxBlocks>>, f32)>(cx, || (Arc::default(), 0.0));

    cx.use_hook(|| {
        let metrics = UseMetrics::new(
            manager.clone(),
            metrics_ref.clone(),
            pane_index,
            editor_index,
        );

        metrics.run_metrics();

        metrics
    })
}
