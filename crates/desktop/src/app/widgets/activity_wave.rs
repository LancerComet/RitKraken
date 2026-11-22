use std::{
    f32::consts::TAU,
    time::{Duration, Instant},
};

use gpui_kit::{
    base::{
        animation::cubic_bezier,
        motion::{MotionStatus, Presence, Transition},
    },
    prelude::*,
    *,
};

use crate::app::theme::Palette;

const SLIDE_DURATION: Duration = Duration::from_millis(320);
const WAVE_BAND_HEIGHT: f32 = 26.0;
const SLIDE_TRAVEL: f32 = 24.0;
const TOOLBAR_INNER_RADIUS: Pixels = px(13.);

#[derive(Clone, Copy, Default)]
struct WaveClock {
    started_at: Option<Instant>,
}

struct WaveLayerSpec {
    base_height: f32,
    harmonics: [(f32, f32, f32); 3],
    phase: f32,
}

const WAVE_LAYERS: [WaveLayerSpec; 3] = [
    WaveLayerSpec {
        base_height: 12.0,
        harmonics: [(5.0, 380.0, 1.55), (3.0, 225.0, -1.10), (1.6, 130.0, 2.30)],
        phase: 0.0,
    },
    WaveLayerSpec {
        base_height: 8.8,
        harmonics: [(4.2, 310.0, -1.85), (2.5, 185.0, 1.35), (1.3, 110.0, -2.65)],
        phase: 2.05,
    },
    WaveLayerSpec {
        base_height: 5.6,
        harmonics: [(3.2, 245.0, 2.25), (1.9, 150.0, -1.70), (1.0, 92.0, 3.10)],
        phase: 4.2,
    },
];

pub(in crate::app) fn toolbar_activity_wave(
    active: bool,
    p: Palette,
    window: &mut Window,
    cx: &mut App,
) -> Option<AnyElement> {
    let motion = Presence::new("toolbar-activity-wave", active)
        .transition(Transition::new(SLIDE_DURATION).ease(cubic_bezier(0.22, 0.58, 0.12, 0.98)))
        .sample(window, cx);
    let clock = window.use_keyed_state("toolbar-activity-wave-clock", cx, |_, _| {
        WaveClock::default()
    });

    let visible = motion.should_render() && (active || motion.progress > 0.001);
    if !visible {
        if clock.read(cx).started_at.is_some() {
            clock.update(cx, |clock, _| clock.started_at = None);
        }
        return None;
    }

    let now = cx.background_executor().now();
    let started_at = match clock.read(cx).started_at {
        Some(started_at) => started_at,
        None => {
            clock.update(cx, |clock, _| clock.started_at = Some(now));
            now
        }
    };

    let phase_secs = if cx.reduce_motion() {
        0.0
    } else {
        if !matches!(motion.status, MotionStatus::Delayed | MotionStatus::Running) {
            window.request_animation_frame();
        }
        now.saturating_duration_since(started_at).as_secs_f32()
    };

    let slide_progress = motion.progress.clamp(0.0, 1.0);
    let wave_canvas = canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            for (spec, colors) in WAVE_LAYERS.iter().zip(p.wave_layers.iter()) {
                let Some(path) = build_wave_path(spec, bounds, phase_secs, slide_progress) else {
                    continue;
                };
                let alpha = (colors.alpha as f32 / 255.0) * (0.4 + 0.6 * slide_progress);
                let fill = linear_gradient(
                    90.0,
                    linear_color_stop(rgb(colors.left).opacity(alpha), 0.0),
                    linear_color_stop(rgb(colors.right).opacity(alpha), 1.0),
                )
                .color_space(ColorSpace::Oklab);
                window.paint_path(path, fill);
            }
        },
    )
    .size_full();

    Some(
        div()
            .absolute()
            .inset_0()
            .rounded(TOOLBAR_INNER_RADIUS)
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(WAVE_BAND_HEIGHT))
                    .child(wave_canvas),
            )
            .into_any_element(),
    )
}

fn sample_height(spec: &WaveLayerSpec, x: f32, time_secs: f32, slide_offset: f32) -> f32 {
    let mut raw = spec.base_height;
    for (index, &(amplitude, wavelength, speed)) in spec.harmonics.iter().enumerate() {
        let angle = x * TAU / wavelength - time_secs * speed + spec.phase * (index as f32 + 1.0);
        raw += amplitude * angle.sin();
    }
    (raw.max(0.5) - slide_offset).max(0.0)
}

fn build_wave_path(
    spec: &WaveLayerSpec,
    bounds: Bounds<Pixels>,
    time_secs: f32,
    slide_progress: f32,
) -> Option<Path<Pixels>> {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    let slide_offset = (1.0 - slide_progress.clamp(0.0, 1.0)) * SLIDE_TRAVEL;
    let steps = (width / 10.0).ceil().clamp(32.0, 192.0) as usize;
    let left = f32::from(bounds.left());
    let bottom = bounds.bottom();
    let st = (point(0., 1.), point(0., 1.), point(0., 1.));

    let mut path = Path::new(bounds.origin);
    path.vertices.reserve(steps * 6);

    for step in 0..steps {
        let x0_local = width * (step as f32 / steps as f32);
        let x1_local = width * ((step + 1) as f32 / steps as f32);
        let h0 = sample_height(spec, x0_local, time_secs, slide_offset).min(height);
        let h1 = sample_height(spec, x1_local, time_secs, slide_offset).min(height);
        if h0 <= 0.0 && h1 <= 0.0 {
            continue;
        }

        let x0 = px(left + x0_local);
        let x1 = px(left + x1_local);
        let b0 = point(x0, bottom);
        let b1 = point(x1, bottom);

        if h0 <= 0.0 {
            let t1 = point(x1, bottom - px(h1));
            path.push_triangle((b0, b1, t1), st);
        } else if h1 <= 0.0 {
            let t0 = point(x0, bottom - px(h0));
            path.push_triangle((b0, b1, t0), st);
        } else {
            let t0 = point(x0, bottom - px(h0));
            let t1 = point(x1, bottom - px(h1));
            path.push_triangle((b0, b1, t1), st);
            path.push_triangle((b0, t1, t0), st);
        }
    }

    if path.vertices.is_empty() {
        return None;
    }
    path.bounds = bounds;
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::{SLIDE_TRAVEL, WAVE_BAND_HEIGHT, WAVE_LAYERS, build_wave_path, sample_height};
    use gpui_kit::{Bounds, point, px, size};

    #[test]
    fn slide_hidden_at_zero_progress() {
        for spec in &WAVE_LAYERS {
            for step in 0..64 {
                let x = step as f32 * 23.0;
                for time in [0.0, 0.7, 2.5, 11.3] {
                    assert_eq!(sample_height(spec, x, time, SLIDE_TRAVEL), 0.0);
                }
            }
        }
    }

    #[test]
    fn wave_heights_stay_within_band_at_full_progress() {
        for spec in &WAVE_LAYERS {
            let mut varied = false;
            let first = sample_height(spec, 120.0, 0.0, 0.0);
            for step in 0..64 {
                let x = step as f32 * 23.0;
                for time in [0.0, 0.7, 2.5, 11.3] {
                    let h = sample_height(spec, x, time, 0.0);
                    assert!(h > 0.0 && h < WAVE_BAND_HEIGHT, "height {h} out of band");
                    if (sample_height(spec, 120.0, time, 0.0) - first).abs() > 1e-3 {
                        varied = true;
                    }
                }
            }
            assert!(varied, "wave height must animate over time");
        }
    }

    #[test]
    fn build_wave_path_tracks_full_bounds_and_skips_when_hidden() {
        let bounds = Bounds::new(point(px(16.), px(28.)), size(px(960.), px(WAVE_BAND_HEIGHT)));
        for spec in &WAVE_LAYERS {
            assert!(build_wave_path(spec, bounds, 1.25, 0.0).is_none());
        }
        for progress in [0.45, 1.0] {
            let any_visible = WAVE_LAYERS
                .iter()
                .filter_map(|spec| build_wave_path(spec, bounds, 1.25, progress))
                .collect::<Vec<_>>();
            assert!(!any_visible.is_empty());
            for path in any_visible {
                assert_eq!(path.bounds, bounds);
                assert!(!path.vertices.is_empty());
            }
        }
    }
}
