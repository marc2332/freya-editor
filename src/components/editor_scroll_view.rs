use std::ops::Range;

use freya::prelude::*;
use freya::prelude::{dioxus_elements, keyboard::Key};

use crate::{
    get_container_size, get_corrected_scroll_position, get_scroll_position_from_cursor,
    get_scrollbar_pos_and_size, is_scrollbar_visible, Axis, SCROLLBAR_SIZE,
};

pub fn get_scroll_position_from_wheel(
    wheel_movement: f32,
    inner_size: f32,
    viewport_size: f32,
    scroll_position: f32,
) -> i32 {
    if viewport_size >= inner_size {
        return 0;
    }

    let new_position = scroll_position + (wheel_movement * 2.0);

    if new_position >= 0.0 && wheel_movement > 0.0 {
        return 0;
    }

    if new_position <= -(inner_size - viewport_size) && wheel_movement < 0.0 {
        return -(inner_size - viewport_size) as i32;
    }

    new_position as i32
}

/// Properties for the EditorScrollView component.
#[derive(Props)]
pub struct EditorScrollViewProps<'a, Builder, BuilderArgs> {
    length: usize,
    item_size: f32,
    #[props(default = "100%".to_string(), into)]
    pub height: String,
    #[props(default = "100%".to_string(), into)]
    pub width: String,
    #[props(default = "0".to_string(), into)]
    pub padding: String,
    #[props(default = true, into)]
    pub show_scrollbar: bool,
    pub offset_y: i32,
    pub offset_x: i32,
    pub onscroll: Option<EventHandler<'a, (Axis, i32)>>,

    builder_args: BuilderArgs,
    builder: Builder,
}

pub fn get_render_range(
    viewport_size: f32,
    scroll_position: f32,
    item_size: f32,
    item_length: f32,
) -> Range<usize> {
    let render_index_start = (-scroll_position) / item_size;
    let potentially_visible_length = viewport_size / item_size;
    let remaining_length = item_length - render_index_start;

    let render_index_end = if remaining_length <= potentially_visible_length {
        item_length
    } else {
        render_index_start + potentially_visible_length
    };

    render_index_start as usize..(render_index_end as usize)
}

/// A controlled ScrollView with virtual scrolling.
#[allow(non_snake_case)]
pub fn EditorScrollView<'a, Builder, BuilderArgs: Clone>(
    cx: Scope<'a, EditorScrollViewProps<'a, Builder, BuilderArgs>>,
) -> Element
where
    Builder: Fn(usize, BuilderArgs) -> LazyNodes<'a, 'a>,
{
    let clicking_shift = use_ref(cx, || false);
    let clicking_alt = use_ref(cx, || false);
    let clicking_scrollbar = use_ref::<Option<(Axis, f64)>>(cx, || None);
    let scrolled_y = cx.props.offset_y;
    let scrolled_x = cx.props.offset_x;
    let onscroll = cx.props.onscroll.as_ref().unwrap();
    let focus = use_focus(cx);
    let (node_ref, size) = use_node(cx);

    let padding = &cx.props.padding;
    let user_container_width = &cx.props.width;
    let user_container_height = &cx.props.height;
    let show_scrollbar = cx.props.show_scrollbar;
    let items_length = cx.props.length;
    let items_size = cx.props.item_size;

    let inner_size = items_size + (items_size * items_length as f32);

    let vertical_scrollbar_is_visible =
        is_scrollbar_visible(show_scrollbar, inner_size, size.area.height());
    let horizontal_scrollbar_is_visible =
        is_scrollbar_visible(show_scrollbar, size.inner.width, size.area.width());

    let container_width = get_container_size(vertical_scrollbar_is_visible);
    let container_height = get_container_size(horizontal_scrollbar_is_visible);

    let corrected_scrolled_y =
        get_corrected_scroll_position(inner_size, size.area.height(), scrolled_y as f32);
    let corrected_scrolled_x =
        get_corrected_scroll_position(size.inner.width, size.area.width(), scrolled_x as f32);

    let (scrollbar_y, scrollbar_height) =
        get_scrollbar_pos_and_size(inner_size, size.area.height(), corrected_scrolled_y);
    let (scrollbar_x, scrollbar_width) =
        get_scrollbar_pos_and_size(size.inner.width, size.area.width(), corrected_scrolled_x);

    // Moves the Y axis when the user scrolls in the container
    let onwheel = move |e: WheelEvent| {
        let speed_multiplier = if *clicking_alt.read() {
            SCROLL_SPEED_MULTIPLIER
        } else {
            1.0
        };

        if !*clicking_shift.read() {
            let wheel_y = e.get_delta_y() as f32 * speed_multiplier;

            let scroll_position_y = get_scroll_position_from_wheel(
                wheel_y,
                inner_size,
                size.area.height(),
                scrolled_y as f32,
            );

            onscroll.call((Axis::Y, scroll_position_y));
        }

        let wheel_x = if *clicking_shift.read() {
            e.get_delta_y() as f32
        } else {
            e.get_delta_x() as f32
        } * speed_multiplier;

        let scroll_position_x = get_scroll_position_from_wheel(
            wheel_x,
            size.inner.width,
            size.area.width(),
            corrected_scrolled_x,
        );

        onscroll.call((Axis::X, scroll_position_x));

        focus.focus();
    };

    // Drag the scrollbars
    let onmouseover = move |e: MouseEvent| {
        let clicking_scrollbar = clicking_scrollbar.read();

        if let Some((Axis::Y, y)) = *clicking_scrollbar {
            let coordinates = e.get_element_coordinates();
            let cursor_y = coordinates.y - y - size.area.min_y() as f64;

            let scroll_position =
                get_scroll_position_from_cursor(cursor_y as f32, inner_size, size.area.height());

            onscroll.call((Axis::Y, scroll_position))
        } else if let Some((Axis::X, x)) = *clicking_scrollbar {
            let coordinates = e.get_element_coordinates();
            let cursor_x = coordinates.x - x - size.area.min_x() as f64;

            let scroll_position = get_scroll_position_from_cursor(
                cursor_x as f32,
                size.inner.width,
                size.area.width(),
            );

            onscroll.call((Axis::X, scroll_position))
        }

        if clicking_scrollbar.is_some() {
            focus.focus();
        }
    };

    let onkeydown = move |e: KeyboardEvent| {
        match &e.key {
            Key::Shift => {
                clicking_shift.set(true);
            }
            Key::Alt => {
                clicking_alt.set(true);
            }
            _ => {
                // TODO: Support other keys with `manage_key_event`
            }
        };
    };

    let onkeyup = |e: KeyboardEvent| {
        if e.key == Key::Shift {
            clicking_shift.set(false);
        } else if e.key == Key::Alt {
            clicking_alt.set(false);
        }
    };

    // Mark the Y axis scrollbar as the one being dragged
    let onmousedown_y = |e: MouseEvent| {
        let coordinates = e.get_element_coordinates();
        *clicking_scrollbar.write() = Some((Axis::Y, coordinates.y));
    };

    // Mark the X axis scrollbar as the one being dragged
    let onmousedown_x = |e: MouseEvent| {
        let coordinates = e.get_element_coordinates();
        *clicking_scrollbar.write() = Some((Axis::X, coordinates.x));
    };

    // Unmark any scrollbar
    let onclick = |_: MouseEvent| {
        if clicking_scrollbar.read().is_some() {
            *clicking_scrollbar.write() = None;
        }
    };

    let horizontal_scrollbar_size = if horizontal_scrollbar_is_visible {
        SCROLLBAR_SIZE
    } else {
        0
    };

    let vertical_scrollbar_size = if vertical_scrollbar_is_visible {
        SCROLLBAR_SIZE
    } else {
        0
    };

    // Calculate from what to what items must be rendered
    let render_range = get_render_range(
        size.area.height(),
        corrected_scrolled_y,
        items_size,
        items_length as f32,
    );

    let is_scrolling_x = clicking_scrollbar
        .read()
        .as_ref()
        .map(|f| f.0 == Axis::X)
        .unwrap_or_default();
    let is_scrolling_y = clicking_scrollbar
        .read()
        .as_ref()
        .map(|f| f.0 == Axis::Y)
        .unwrap_or_default();

    render!(
        rect {
            overflow: "clip",
            direction: "horizontal",
            width: "{user_container_width}",
            height: "{user_container_height}",
            onclick: onclick,
            onglobalmouseover: onmouseover,
            onkeydown: onkeydown,
            onkeyup: onkeyup,
            rect {
                direction: "vertical",
                width: "{container_width}",
                height: "{container_height}",
                rect {
                    overflow: "clip",
                    padding: "{padding}",
                    height: "100%",
                    width: "100%",
                    direction: "vertical",
                    offset_x: "{corrected_scrolled_x}",
                    reference: node_ref,
                    onwheel: onwheel,
                    render_range.map(|i| (cx.props.builder)(i, cx.props.builder_args.clone()))
                }
                ScrollBar {
                    width: "100%",
                    height: "{horizontal_scrollbar_size}",
                    offset_x: "{scrollbar_x}",
                    clicking_scrollbar: is_scrolling_x,
                    ScrollThumb {
                        clicking_scrollbar: is_scrolling_x,
                        onmousedown: onmousedown_x,
                        width: "{scrollbar_width}",
                        height: "100%",
                    },
                }
            }
            ScrollBar {
                width: "{vertical_scrollbar_size}",
                height: "100%",
                offset_y: "{scrollbar_y}",
                clicking_scrollbar: is_scrolling_y,
                ScrollThumb {
                    clicking_scrollbar: is_scrolling_y,
                    onmousedown: onmousedown_y,
                    width: "100%",
                    height: "{scrollbar_height}",
                }
            }
        }
    )
}
