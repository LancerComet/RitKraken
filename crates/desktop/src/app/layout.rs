//! Remembers the window placement and the sizes the user last dragged
//! sidebars, panes and table columns to, across restarts.

use std::{collections::BTreeMap, time::Duration};

use gpui_kit::{
    base::{ResizablePanelEvent, ResizableState},
    *,
};

use super::settings::{read_state, write_state};

const LAYOUT_FILE: &str = "layout.json";
/// Window moves and resizes arrive continuously while dragging; write once
/// the user has paused instead of on every frame.
const SAVE_DELAY: Duration = Duration::from_millis(500);
/// The smallest window the app allows. Smaller reported bounds come from a
/// minimized or closing window, not from the user's layout.
pub const MIN_WINDOW_SIZE: Size<Pixels> = size(px(920.), px(560.));
/// How much of a saved window must still be on a display to reuse its position.
const VISIBLE_EDGE: Pixels = px(120.);

#[derive(Default)]
struct LayoutMemory {
    panes: BTreeMap<String, Vec<f32>>,
    window: Option<WindowBounds>,
    /// The group state the user last dragged for each key. Dragging one group
    /// can rescale a neighbouring one, so sizes are read again when saving to
    /// keep what was on screen.
    dragged: BTreeMap<&'static str, WeakEntity<ResizableState>>,
    generation: u64,
}

impl Global for LayoutMemory {}

impl LayoutMemory {
    fn read(value: &serde_json::Value) -> Self {
        let panes = value["panes"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(key, sizes)| {
                let sizes = sizes
                    .as_array()?
                    .iter()
                    .map(|size| size.as_f64().map(|size| size as f32))
                    .collect::<Option<Vec<_>>>()?;
                sizes
                    .iter()
                    .all(|size| size.is_finite() && *size > 0.)
                    .then(|| (key.clone(), sizes))
            })
            .collect();
        Self {
            panes,
            window: read_window(&value["window"]),
            ..Self::default()
        }
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "panes": self.panes,
            "window": self.window.map(window_json),
        })
    }
}

fn read_window(value: &serde_json::Value) -> Option<WindowBounds> {
    let number = |key: &str| value[key].as_f64().filter(|number| number.is_finite());
    let bounds = Bounds::new(
        point(px(number("x")? as f32), px(number("y")? as f32)),
        size(px(number("width")? as f32), px(number("height")? as f32)),
    );
    if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
        return None;
    }
    Some(match value["state"].as_str()? {
        "windowed" => WindowBounds::Windowed(bounds),
        "maximized" => WindowBounds::Maximized(bounds),
        _ => return None,
    })
}

fn window_json(window: WindowBounds) -> serde_json::Value {
    let bounds = window.get_bounds();
    serde_json::json!({
        // A fullscreen window reopens maximized; its restore bounds are kept.
        "state": if matches!(window, WindowBounds::Windowed(_)) { "windowed" } else { "maximized" },
        "x": f32::from(bounds.origin.x),
        "y": f32::from(bounds.origin.y),
        "width": f32::from(bounds.size.width),
        "height": f32::from(bounds.size.height),
    })
}

pub(super) fn init(cx: &mut App) {
    let memory = read_state(LAYOUT_FILE)
        .map(|value| LayoutMemory::read(&value))
        .unwrap_or_default();
    cx.set_global(memory);
    cx.on_app_quit(|cx| {
        save_now(cx);
        async {}
    })
    .detach();
}

/// The placement the window had when the app last ran, if it still fits on a
/// connected display.
pub fn saved_window_bounds(cx: &App) -> Option<WindowBounds> {
    let window = cx.try_global::<LayoutMemory>()?.window?;
    let mut bounds = window.get_bounds();
    bounds.size = bounds.size.max(&MIN_WINDOW_SIZE);
    let displays: Vec<_> = cx
        .displays()
        .iter()
        .map(|display| display.bounds())
        .collect();
    fits_a_display(bounds, &displays).then_some(match window {
        WindowBounds::Windowed(_) => WindowBounds::Windowed(bounds),
        _ => WindowBounds::Maximized(bounds),
    })
}

fn is_user_layout(window: Size<Pixels>) -> bool {
    window.width >= MIN_WINDOW_SIZE.width && window.height >= MIN_WINDOW_SIZE.height
}

fn fits_a_display(bounds: Bounds<Pixels>, displays: &[Bounds<Pixels>]) -> bool {
    displays.iter().any(|display| {
        let shared = display.intersect(&bounds).size;
        shared.width >= VISIBLE_EDGE && shared.height >= VISIBLE_EDGE
    })
}

/// Records the window placement whenever the window moves or resizes.
pub(super) fn remember_window<V: 'static>(window: &mut Window, cx: &mut Context<V>) {
    cx.observe_window_bounds(window, |_, window, cx| {
        let bounds = window.window_bounds();
        if !is_user_layout(bounds.get_bounds().size) {
            return;
        }
        let memory = cx.default_global::<LayoutMemory>();
        if memory.window != Some(bounds) {
            memory.window = Some(bounds);
            schedule_save(cx);
        }
    })
    .detach();
}

/// A resizable group state whose sizes are remembered under `key` once the
/// user has dragged one of its handles.
pub(super) fn remembered_panes<V: 'static>(
    key: &'static str,
    cx: &mut Context<V>,
) -> Entity<ResizableState> {
    let state = cx.new(|_| ResizableState::default());
    cx.subscribe(&state, move |_, state, _: &ResizablePanelEvent, cx| {
        let sizes = current_sizes(&state, cx);
        let memory = cx.default_global::<LayoutMemory>();
        memory.dragged.insert(key, state.downgrade());
        if let Some(sizes) = sizes {
            memory.panes.insert(key.to_owned(), sizes);
        }
        schedule_save(cx);
    })
    .detach();
    state
}

/// The size panel `index` of the `key` group had when the user last resized it.
pub(super) fn remembered_size(key: &str, index: usize, cx: &App) -> Option<Pixels> {
    cx.try_global::<LayoutMemory>()?
        .panes
        .get(key)?
        .get(index)
        .map(|size| px(*size))
}

fn schedule_save(cx: &mut App) {
    let memory = cx.default_global::<LayoutMemory>();
    memory.generation += 1;
    let generation = memory.generation;
    let timer = cx.background_executor().timer(SAVE_DELAY);
    cx.spawn(async move |cx| {
        timer.await;
        cx.update(|cx| {
            if cx
                .try_global::<LayoutMemory>()
                .is_some_and(|memory| memory.generation == generation)
            {
                save_now(cx);
            }
        })
    })
    .detach();
}

fn current_sizes(state: &Entity<ResizableState>, cx: &App) -> Option<Vec<f32>> {
    let sizes: Vec<f32> = state
        .read(cx)
        .sizes()
        .iter()
        .map(|size| f32::from(*size))
        .collect();
    (!sizes.is_empty() && sizes.iter().all(|size| size.is_finite() && *size > 0.)).then_some(sizes)
}

fn save_now(cx: &mut App) {
    let dragged: Vec<_> = cx
        .default_global::<LayoutMemory>()
        .dragged
        .iter()
        .map(|(key, state)| (*key, state.clone()))
        .collect();
    let current: Vec<_> = dragged
        .into_iter()
        .filter_map(|(key, state)| Some((key, current_sizes(&state.upgrade()?, cx)?)))
        .collect();
    let memory = cx.default_global::<LayoutMemory>();
    for (key, sizes) in current {
        memory.panes.insert(key.to_owned(), sizes);
    }
    write_state(LAYOUT_FILE, &memory.json());
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, WindowBounds, point, px, size};

    use super::{LayoutMemory, fits_a_display, is_user_layout, read_window, window_json};

    #[test]
    fn layout_round_trips_and_drops_invalid_entries() {
        let mut memory = LayoutMemory::default();
        memory.panes.insert("workspace".into(), vec![900., 420.5]);
        memory.window = Some(WindowBounds::Maximized(Bounds::new(
            point(px(-1200.), px(40.)),
            size(px(1100.), px(700.)),
        )));
        let saved = LayoutMemory::read(&memory.json());
        assert_eq!(saved.panes, memory.panes);
        assert_eq!(saved.window, memory.window);

        let invalid = LayoutMemory::read(&serde_json::json!({
            "panes": {"a": [0.0, 10.0], "b": ["10"], "c": [12.0]},
            "window": {"state": "minimized", "x": 0, "y": 0, "width": 800, "height": 600},
        }));
        assert_eq!(invalid.panes.keys().collect::<Vec<_>>(), ["c"]);
        assert_eq!(invalid.window, None);
        assert_eq!(LayoutMemory::read(&serde_json::Value::Null).window, None);
    }

    #[test]
    fn saved_windows_must_remain_reachable_on_a_display() {
        let screen = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.)));
        let window = |x: f32, y: f32| Bounds::new(point(px(x), px(y)), size(px(1000.), px(600.)));
        assert!(fits_a_display(window(100., 100.), &[screen]));
        assert!(fits_a_display(window(1700., 900.), &[screen]));
        assert!(!fits_a_display(window(1850., 100.), &[screen]));
        assert!(!fits_a_display(window(-30000., -30000.), &[screen]));
        assert!(!fits_a_display(window(100., 100.), &[]));
    }

    #[test]
    fn minimized_and_closing_window_bounds_are_not_remembered() {
        assert!(is_user_layout(size(px(1300.), px(850.))));
        assert!(!is_user_layout(size(px(-16.), px(-39.))));
        assert!(!is_user_layout(size(px(160.), px(28.))));
    }

    #[test]
    fn fullscreen_windows_reopen_maximized() {
        let bounds = Bounds::new(point(px(10.), px(20.)), size(px(1000.), px(600.)));
        let json = window_json(WindowBounds::Fullscreen(bounds));
        assert_eq!(read_window(&json), Some(WindowBounds::Maximized(bounds)));
    }
}
