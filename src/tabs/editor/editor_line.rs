use freya::prelude::*;
use lsp_types::{
    Hover, HoverParams, Position, TextDocumentIdentifier, TextDocumentPositionParams, Url,
    WorkDoneProgressParams,
};

use crate::hooks;
use crate::tabs::editor::hooks::LspAction;
use crate::tabs::editor::hover_box::HoverBox;
use crate::tabs::editor::lsp::HoverToText;
use crate::{hooks::UseDebouncer, utils::create_paragraph};

use super::hooks::UseLsp;

pub type BuilderProps = (
    TextCursor,
    hooks::UseMetrics,
    hooks::UseEdit,
    UseLsp,
    Url,
    Rope,
    UseRef<Option<(u32, Hover)>>,
    UseRef<CursorPoint>,
    UseDebouncer,
);

#[derive(Props)]
pub struct EditorLineProps {
    options: BuilderProps,
    line_index: usize,
    font_size: f32,
    line_height: f32,
}

impl PartialEq for EditorLineProps {
    fn eq(&self, other: &Self) -> bool {
        self.options.0 == other.options.0
            && self.options.2 == other.options.2
            && self.options.4 == other.options.4
            && self.line_index == other.line_index
            && self.font_size == other.font_size
            && self.line_height == other.line_height
    }
}

#[allow(non_snake_case)]
pub fn EditorLine(cx: Scope<EditorLineProps>) -> Element {
    let EditorLineProps {
        options,
        line_index,
        font_size,
        line_height,
    } = cx.props;
    let (cursor, metrics, editable, lsp, file_uri, rope, hover_location, cursor_coords, debouncer) =
        options;

    let onmousedown = {
        to_owned![editable];
        move |e: MouseEvent| {
            editable.process_event(&EditableEvent::MouseDown(e.data, *line_index));
        }
    };

    let onmouseleave = |_| {
        lsp.send(LspAction::Clear);
    };

    let onmouseover = {
        to_owned![editable, file_uri, lsp, cursor_coords, hover_location];
        move |e: MouseEvent| {
            let line_str = rope.line(*line_index).to_string();
            let coords = e.get_element_coordinates();
            let data = e.data;

            editable.process_event(&EditableEvent::MouseOver(data, *line_index));

            // Optimization: Re run the component only when the hover box is shown
            // otherwise just update the coordinates silently
            if hover_location.read().is_some() {
                *cursor_coords.write() = coords;
            } else {
                *cursor_coords.write_silent() = coords;
            }

            let paragraph = create_paragraph(&line_str, *font_size);

            if (coords.x as f32) < paragraph.max_intrinsic_width() {
                to_owned![cursor_coords, file_uri, lsp, line_index];
                debouncer.action(move || {
                    let coords = cursor_coords.read();
                    let glyph = paragraph
                        .get_glyph_position_at_coordinate((coords.x as i32, coords.y as i32));

                    lsp.send(LspAction::Hover(HoverParams {
                        text_document_position_params: TextDocumentPositionParams {
                            text_document: TextDocumentIdentifier {
                                uri: file_uri.clone(),
                            },
                            position: Position::new(line_index as u32, glyph.position as u32),
                        },
                        work_done_progress_params: WorkDoneProgressParams::default(),
                    }));
                });
            } else {
                lsp.send(LspAction::Clear);
            }
        }
    };

    let gutter_width = font_size * 3.0;
    let (syntax_blocks, width) = &*metrics.get();
    let line = {
        let blocks = syntax_blocks.lock().unwrap();
        blocks.get_line(*line_index).to_vec()
    };
    let highlights_attr = editable.highlights_attr(cx, *line_index);

    let is_line_selected = cursor.row() == *line_index;

    // Only show the cursor in the active line
    let character_index = if is_line_selected {
        cursor.col().to_string()
    } else {
        "none".to_string()
    };

    // Only highlight the active line
    let (line_background, gutter_color) = if is_line_selected {
        ("rgb(37, 37, 37)", "rgb(200, 200, 200)")
    } else {
        ("", "rgb(150, 150, 150)")
    };

    render!(
        rect {
            height: "{line_height}",
            direction: "horizontal",
            background: "{line_background}",
            if let Some((line, hover)) = hover_location.read().as_ref() {
                if *line == *line_index as u32 {
                    if let Some(content) = hover.hover_to_text() {
                        let cursor_coords = cursor_coords.read();
                        let offset_x = cursor_coords.x  as f32 + gutter_width;
                        Some(rsx!(
                            rect {
                                width: "0",
                                height: "0",
                                offset_y: "{line_height}",
                                offset_x: "{offset_x}",
                                HoverBox {
                                    content: content
                                }
                            }
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
            rect {
                width: "{gutter_width}",
                height: "100%",
                direction: "horizontal",
                label {
                    width: "100%",
                    text_align: "center",
                    font_size: "{font_size}",
                    color: "{gutter_color}",
                    "{line_index + 1} "
                }
            }
            paragraph {
                min_width: "calc(100% - {gutter_width})",
                width: "{width}",
                cursor_index: "{character_index}",
                cursor_color: "white",
                max_lines: "1",
                cursor_mode: "editable",
                cursor_id: "{line_index}",
                onmousedown: onmousedown,
                onmouseover: onmouseover,
                onmouseleave: onmouseleave,
                highlights: highlights_attr,
                highlight_color: "rgb(65, 65, 65)",
                direction: "horizontal",
                font_size: "{font_size}",
                font_family: "Jetbrains Mono",
                line.iter().enumerate().map(|(i, (syntax_type, text))| {
                    let text = rope.slice(text.clone());
                    rsx!(
                        text {
                            key: "{i}",
                            color: "{syntax_type.color()}",
                            "{text}"
                        }
                    )
                })
            }
        }
    )
}
