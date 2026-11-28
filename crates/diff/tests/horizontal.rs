use gpui::{Bounds, point, px, size};
use ritkraken_diff::{horizontal_auto_scroll_delta, step_horizontal_offset};

fn viewport(x: f32, width: f32) -> Bounds<gpui::Pixels> {
    Bounds::new(point(px(x), px(0.)), size(px(width), px(100.)))
}

#[test]
fn dead_zone_inside_viewport_returns_none() {
    let bounds = viewport(100., 400.);
    assert_eq!(horizontal_auto_scroll_delta(px(300.), bounds), None);
    // Just inside the 16px inner zones on either side.
    assert_eq!(horizontal_auto_scroll_delta(px(117.), bounds), None);
    assert_eq!(horizontal_auto_scroll_delta(px(483.), bounds), None);
}

#[test]
fn right_edge_scrolls_positive_and_ramps_to_max_speed() {
    let bounds = viewport(100., 400.);
    // Right trigger at 500 - 16 = 484.
    let near = horizontal_auto_scroll_delta(px(485.), bounds).unwrap();
    assert!(near > px(12.) && near < px(13.));
    let middle = horizontal_auto_scroll_delta(px(532.), bounds).unwrap();
    assert!(middle > near && middle < px(64.));
    // Full speed at inner + ramp (16 + 80 = 96px) past the trigger...
    let max = horizontal_auto_scroll_delta(px(580.), bounds).unwrap();
    assert_eq!(max, px(64.));
    // ...and clamped beyond it.
    assert_eq!(horizontal_auto_scroll_delta(px(700.), bounds), Some(px(64.)));
}

#[test]
fn left_edge_scrolls_negative_and_ramps_to_max_speed() {
    let bounds = viewport(100., 400.);
    // Left trigger at 100 + 16 = 116.
    let near = horizontal_auto_scroll_delta(px(115.), bounds).unwrap();
    assert!(near < px(-12.) && near > px(-13.));
    let middle = horizontal_auto_scroll_delta(px(68.), bounds).unwrap();
    assert!(middle < near && middle > px(-64.));
    let max = horizontal_auto_scroll_delta(px(20.), bounds).unwrap();
    assert_eq!(max, px(-64.));
    assert_eq!(horizontal_auto_scroll_delta(px(-200.), bounds), Some(px(-64.)));
}

#[test]
fn edge_directions_are_symmetric() {
    let bounds = viewport(100., 400.);
    for (left_x, right_x) in [(110., 490.), (90., 510.), (0., 600.)] {
        let left = horizontal_auto_scroll_delta(px(left_x), bounds).unwrap();
        let right = horizontal_auto_scroll_delta(px(right_x), bounds).unwrap();
        assert_eq!(left, -right);
    }
}

#[test]
fn narrow_viewport_keeps_a_center_dead_zone() {
    // Width 20 shrinks the inner zone to 20 / 4 = 5px per side.
    let bounds = viewport(0., 20.);
    assert_eq!(horizontal_auto_scroll_delta(px(10.), bounds), None);
    assert!(horizontal_auto_scroll_delta(px(16.), bounds).unwrap() > px(0.));
    assert!(horizontal_auto_scroll_delta(px(4.), bounds).unwrap() < px(0.));
}

#[test]
fn zero_width_viewport_returns_none() {
    let bounds = viewport(0., 0.);
    assert_eq!(horizontal_auto_scroll_delta(px(0.), bounds), None);
    assert_eq!(horizontal_auto_scroll_delta(px(1000.), bounds), None);
}

#[test]
fn step_scrolls_right_and_clamps_at_max_offset() {
    // Positive delta scrolls toward the end of long lines: offsets go negative.
    assert_eq!(
        step_horizontal_offset(px(0.), px(200.), px(30.)),
        Some(px(-30.))
    );
    assert_eq!(
        step_horizontal_offset(px(-190.), px(200.), px(30.)),
        Some(px(-200.))
    );
    // Already at the end: nothing left to scroll.
    assert_eq!(step_horizontal_offset(px(-200.), px(200.), px(30.)), None);
}

#[test]
fn step_scrolls_left_and_clamps_at_zero() {
    assert_eq!(
        step_horizontal_offset(px(-150.), px(200.), px(-30.)),
        Some(px(-120.))
    );
    assert_eq!(
        step_horizontal_offset(px(-10.), px(200.), px(-30.)),
        Some(px(0.))
    );
    assert_eq!(step_horizontal_offset(px(0.), px(200.), px(-30.)), None);
}

#[test]
fn step_returns_none_without_scrollable_overflow() {
    assert_eq!(step_horizontal_offset(px(0.), px(0.), px(30.)), None);
    assert_eq!(step_horizontal_offset(px(0.), px(0.), px(-30.)), None);
    // A negative max is treated as no overflow.
    assert_eq!(step_horizontal_offset(px(0.), px(-50.), px(30.)), None);
}

#[test]
fn step_recovers_from_out_of_range_offsets() {
    // An offset beyond the range is clamped before stepping, so even scrolling
    // toward that edge moves the handle back inside the range.
    assert_eq!(
        step_horizontal_offset(px(-250.), px(200.), px(-10.)),
        Some(px(-190.))
    );
    assert_eq!(
        step_horizontal_offset(px(20.), px(200.), px(30.)),
        Some(px(-30.))
    );
    // Scrolling further out of range is a no-op.
    assert_eq!(step_horizontal_offset(px(-250.), px(200.), px(30.)), None);
    assert_eq!(step_horizontal_offset(px(20.), px(200.), px(-10.)), None);
}
