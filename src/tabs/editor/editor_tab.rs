use std::path::PathBuf;
use std::time::Duration;

use crate::components::*;
use crate::editor_manager::EditorView;
use crate::hooks::*;
use crate::lsp::LanguageId;
use crate::lsp::LspConfig;
use crate::tabs::editor::hooks::use_lsp;
use crate::tabs::editor::EditorLine;

use freya::prelude::events::KeyboardEvent;
use freya::prelude::keyboard::Key;
use freya::prelude::keyboard::Modifiers;
use freya::prelude::*;

use freya_node_state::Parse;
use lsp_types::Url;
use skia_safe::textlayout::ParagraphBuilder;
use skia_safe::textlayout::ParagraphStyle;
use skia_safe::textlayout::TextAlign;
use skia_safe::textlayout::TextStyle;
use skia_safe::Color;
use skia_safe::Font;
use skia_safe::FontStyle;
use skia_safe::Paint;
use skia_safe::PaintStyle;
use skia_safe::Typeface;

static LINES_JUMP_ALT: usize = 5;
static LINES_JUMP_CONTROL: usize = 3;

#[derive(Props, PartialEq)]
pub struct EditorTabProps {
    pub panel_index: usize,
    pub editor_index: usize,
    pub language_id: LanguageId,
    pub root_path: PathBuf,
}

#[allow(non_snake_case)]
pub fn EditorTab(cx: Scope<EditorTabProps>) -> Element {
    let lsp_config = LspConfig::new(cx.props.root_path.clone(), cx.props.language_id);
    let manager = use_manager(
        cx,
        SubscriptionModel::follow_tab(cx.props.panel_index, cx.props.editor_index),
    );
    let debouncer = use_debouncer(cx, Duration::from_millis(300));
    let hover_location = use_ref(cx, || None);
    let metrics = use_metrics(cx, manager, cx.props.panel_index, cx.props.editor_index);
    let editable = use_edit(
        cx,
        manager,
        cx.props.panel_index,
        cx.props.editor_index,
        metrics,
    );
    let cursor_coords = use_ref(cx, CursorPoint::default);
    let scroll_offsets = use_ref(cx, || (0, 0));
    let lsp = use_lsp(
        cx,
        cx.props.language_id,
        cx.props.panel_index,
        cx.props.editor_index,
        &lsp_config,
        manager,
        hover_location,
    );

    // Focus editor when created
    cx.use_hook(|| {
        let mut manager = manager.write();
        manager.set_focused_panel(cx.props.panel_index);
        manager
            .panel_mut(cx.props.panel_index)
            .set_active_tab(cx.props.editor_index);
    });

    let onscroll = move |(axis, scroll): (Axis, i32)| match axis {
        Axis::X => {
            if scroll_offsets.read().0 != scroll {
                scroll_offsets.write().0 = scroll
            }
        }
        Axis::Y => {
            if scroll_offsets.read().1 != scroll {
                scroll_offsets.write().1 = scroll
            }
        }
    };

    let onglobalclick = {
        to_owned![editable, manager];
        move |_: MouseEvent| {
            let is_panel_focused = manager.current().focused_panel() == cx.props.panel_index;

            if is_panel_focused {
                editable.process_event(&EditableEvent::Click);
            }
        }
    };

    let onclick = {
        to_owned![manager];
        move |_: MouseEvent| {
            let (is_code_editor_view_focused, is_editor_focused) = {
                let manager_ref = manager.current();
                let panel = manager_ref.panel(cx.props.panel_index);
                let is_code_editor_view_focused =
                    *manager_ref.focused_view() == EditorView::CodeEditor;
                let is_editor_focused = manager_ref.focused_panel() == cx.props.panel_index
                    && panel.active_tab() == Some(cx.props.editor_index);
                (is_code_editor_view_focused, is_editor_focused)
            };

            if !is_code_editor_view_focused {
                let mut manager = manager.global_write();
                manager.set_focused_view(EditorView::CodeEditor);
            }

            if !is_editor_focused {
                let mut manager = manager.global_write();
                manager.set_focused_panel(cx.props.panel_index);
                manager
                    .panel_mut(cx.props.panel_index)
                    .set_active_tab(cx.props.editor_index);
            }
        }
    };

    let manager_ref = manager.current();
    let cursor_attr = editable.cursor_attr(cx);
    let font_size = manager_ref.font_size();
    let line_height = manager_ref.line_height();
    let manual_line_height = (font_size * line_height).floor();
    let panel = manager_ref.panel(cx.props.panel_index);

    let onkeydown = {
        to_owned![editable, manager];
        move |e: KeyboardEvent| {
            let (is_panel_focused, is_editor_focused) = {
                let manager_ref = manager.current();
                let panel = manager_ref.panel(cx.props.panel_index);
                let is_panel_focused = manager_ref.focused_panel() == cx.props.panel_index;
                let is_editor_focused = *manager_ref.focused_view() == EditorView::CodeEditor
                    && panel.active_tab() == Some(cx.props.editor_index);
                (is_panel_focused, is_editor_focused)
            };

            if is_panel_focused && is_editor_focused {
                let current_scroll = scroll_offsets.read().1;
                let lines_jump = (manual_line_height * LINES_JUMP_ALT as f32).ceil() as i32;
                let min_height =
                    -(metrics.get().0.lock().unwrap().len() as f32 * manual_line_height) as i32;
                let max_height = 0; // TODO, this should be the height of the viewport

                let events = match &e.key {
                    Key::ArrowUp if e.modifiers.contains(Modifiers::ALT) => {
                        let jump = (current_scroll + lines_jump).clamp(min_height, max_height);
                        scroll_offsets.write().1 = jump;
                        (0..LINES_JUMP_ALT)
                            .map(|_| EditableEvent::KeyDown(e.data.clone()))
                            .collect::<Vec<EditableEvent>>()
                    }
                    Key::ArrowDown if e.modifiers.contains(Modifiers::ALT) => {
                        let jump = (current_scroll - lines_jump).clamp(min_height, max_height);
                        scroll_offsets.write().1 = jump;
                        (0..LINES_JUMP_ALT)
                            .map(|_| EditableEvent::KeyDown(e.data.clone()))
                            .collect::<Vec<EditableEvent>>()
                    }
                    Key::ArrowDown | Key::ArrowUp if e.modifiers.contains(Modifiers::CONTROL) => (0
                        ..LINES_JUMP_CONTROL)
                        .map(|_| EditableEvent::KeyDown(e.data.clone()))
                        .collect::<Vec<EditableEvent>>(),
                    _ => {
                        vec![EditableEvent::KeyDown(e.data)]
                    }
                };

                for event in events {
                    editable.process_event(&event);
                }
            }
        }
    };

    let editor = panel.tab(cx.props.editor_index).as_text_editor().unwrap();
    let path = editor.path();
    let cursor = editor.cursor();
    let file_uri = Url::from_file_path(path).unwrap();

    let editor_scroll = scroll_offsets.read().1 as f32;

    let minimap_scroll = if editor_scroll < -600.0 {
        editor_scroll + 600.0
    } else {
        0.0
    };

    render!(
        rect {
            width: "100%",
            height: "100%",
            onkeydown: onkeydown,
            onglobalclick: onglobalclick,
            onclick: onclick,
            cursor_reference: cursor_attr,
            direction: "horizontal",
            background: "rgb(40, 40, 40)",
            padding: "5 0 0 5",
            EditorScrollView {
                width: "calc(100% - 150)",
                offset_x: scroll_offsets.read().0,
                offset_y: scroll_offsets.read().1,
                onscroll: onscroll,
                length: metrics.get().0.lock().unwrap().len(),
                item_size: manual_line_height,
                builder_args: (cursor.clone(), metrics.clone(), editable.clone(), lsp.clone(), file_uri, editor.rope().clone(), hover_location.clone(), cursor_coords.clone(), debouncer.clone()),
                builder: move |i, options| rsx!(
                    EditorLine {
                        key: "{i}",
                        line_index: i,
                        options: options,
                        font_size: font_size,
                        line_height: manual_line_height,
                    }
                )
            }

            Minimap {
                scroll_offsets: scroll_offsets.clone(),
                editable: editable,
                rope: editor.rope().clone()
            }
        }
    )
}

#[derive(Props)]
pub struct MinimapProps {
    scroll_offsets: UseRef<(i32, i32)>,
    editable: UseEdit,
    rope: Rope,
}

impl PartialEq for MinimapProps {
    fn eq(&self, other: &Self) -> bool {
        self.scroll_offsets == other.scroll_offsets
    }
}

#[allow(non_snake_case)]
fn Minimap(cx: Scope<MinimapProps>) -> Element {
    let metrics = &cx.props.editable.metrics;

    let canvas = use_canvas(cx, (&cx.props.scroll_offsets.read().1,), |(scroll_y,)| {
        let blocks = metrics.get().0.clone();
        let rope = cx.props.rope.clone();
        Box::new(move |canvas, font_collection, area| {
            canvas.translate((area.min_x(), area.min_y()));

            let mut paragraph_style = ParagraphStyle::default();
            let mut text_style = TextStyle::new();
            text_style.set_font_size(3.0);
            paragraph_style.set_text_style(&text_style);

            let items_len = rope.len_lines();
            let editor_inner_size = (17.0 * 1.2) + ((17.0 * 1.2) * items_len as f32);

            let editor_corrected_scrolled_y =
                get_corrected_scroll_position(editor_inner_size, area.height(), scroll_y as f32);

            let editor_range = get_render_range(
                area.height(),
                editor_corrected_scrolled_y,
                17.0,
                items_len as f32,
            );

            let minimap_inner_size = 3.0 * items_len as f32;

            let editor_position_percentage =
                (-editor_corrected_scrolled_y / editor_inner_size) * 100.0;
            let minimap_position = -(minimap_inner_size * (editor_position_percentage / 100.0));

            let minimap_corrected_scrolled_y =
                get_corrected_scroll_position(minimap_inner_size, area.height(), minimap_position);

            let minimap_range = get_render_range(
                area.height(),
                minimap_corrected_scrolled_y,
                3.0,
                items_len as f32,
            );

            let blocks = blocks.lock().unwrap();

            for (i, n) in minimap_range.enumerate() {
                let y = i as f32 * 3.0;
                let mut paragrap_builder = ParagraphBuilder::new(&paragraph_style, font_collection);
                let line = blocks.get_line(n);
                for (syntax, word) in line {
                    let mut text_style = TextStyle::new();
                    text_style.set_color(Color::parse(syntax.color()).unwrap());
                    text_style.set_font_size(3.0);
                    text_style.set_font_families(&["Jetbrains Mono"]);
                    paragrap_builder.push_style(&text_style);
                    let text = word.to_string(&rope);
                    paragrap_builder.add_text(text);
                }
                let mut paragraph = paragrap_builder.build();

                paragraph.layout(area.width());
                paragraph.paint(canvas, (0.0, y));
            }

            let start_y = editor_range.start as f32 * 3.0;
            let end_y = editor_range.end as f32 * 3.0;

            let mut paint = Paint::default();
            paint.set_style(PaintStyle::Fill);
            paint.set_color(Color::from_argb(100, 255, 255, 255));

            canvas.draw_rect(
                skia_safe::Rect::new(0.0, start_y, area.width(), end_y),
                &paint,
            );

            canvas.restore();
        })
    });

    render!(rect {
        width: "100%",
        height: "100%",
        background: "transparent",
        canvas_reference: canvas.attribute(cx)
    })
}
