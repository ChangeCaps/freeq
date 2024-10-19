use filter::{Filter, FilterState};
use ori_vst::prelude::*;

mod filter;

#[derive(Params)]
pub struct FreeqParams {
    #[param(group)]
    filters: [Filter; 10],
}

pub struct Freeq {
    params: FreeqParams,
    filters: [[FilterState; 10]; 2],
}

impl VstPlugin for Freeq {
    fn info() -> Info {
        Info {
            uuid: uuid!("dab90007-3947-458d-b1b8-7c1b82b72146"),
            name: String::from("FreeQ"),
            vendor: String::from("Hjalte Nannestad"),
            version: String::from(env!("CARGO_PKG_VERSION")),
            url: String::from(env!("CARGO_PKG_HOMEPAGE")),
            email: String::new(),
        }
    }

    fn layout(_inputs: &[u32], _outputs: &[u32]) -> Option<AudioLayout> {
        let layout = AudioLayout::new()
            .with_input(AudioPort::new(2))
            .with_output(AudioPort::new(2));

        Some(layout)
    }

    fn window() -> Window {
        Window::new().title("FreeQ").size(600, 400)
    }

    fn new() -> Self {
        Self {
            params: FreeqParams {
                filters: [
                    Filter::new(0, 10),
                    Filter::new(1, 10),
                    Filter::new(2, 10),
                    Filter::new(3, 10),
                    Filter::new(4, 10),
                    Filter::new(5, 10),
                    Filter::new(6, 10),
                    Filter::new(7, 10),
                    Filter::new(8, 10),
                    Filter::new(9, 10),
                ],
            },
            filters: Default::default(),
        }
    }

    fn params(&mut self) -> &mut dyn Params {
        &mut self.params
    }

    fn ui(&mut self) -> impl View<Self> + 'static {
        curve_view(self)
    }

    fn activate(&mut self, _audio_layout: &AudioLayout, _buffer_layout: &BufferLayout) -> Activate {
        Activate::new()
    }

    fn process(
        &mut self,
        buffer: &mut Buffer<'_>,
        _aux_buffers: &mut [Buffer<'_>],
        layout: BufferLayout,
    ) -> Process {
        for (i, filter) in self.params.filters.iter().enumerate() {
            for filters in self.filters.iter_mut() {
                filters[i].set_params(filter, layout.sample_rate)
            }
        }

        for samples in buffer.iter_samples() {
            for (sample, filters) in samples.zip(self.filters.iter_mut()) {
                for filter in filters.iter_mut() {
                    *sample = filter.process(*sample);
                }
            }
        }

        Process::Done
    }
}

vst3!(Freeq);

const CONTROL_RADIUS: f32 = 6.0;

const GAIN_LINES: &[f32] = &[-18.0, -12.0, -6.0, 0.0, 6.0, 12.0, 18.0];

const FREQ_LINES: &[f32] = &[
    20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 200.0, 300.0, 400.0, 500.0, 600.0,
    700.0, 800.0, 900.0, 1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0,
    10000.0, 20000.0,
];

const FREQ_TEXT: &[f32] = &[
    20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0,
];

#[derive(Default)]
struct CurveView {
    selected: Option<usize>,
}

fn curve_view(_eqo: &mut Freeq) -> impl View<Freeq> {
    with_state_default(|_state, _eqo| {
        let view = painter(|cx, (_state, eqo): &mut (CurveView, Freeq)| {
            let styles = cx.styles();
            let line_color = styles.get(Theme::OUTLINE).unwrap();
            let label_color = styles.get(Theme::CONTRAST_LOW).unwrap();
            let contrast_color = styles.get(Theme::CONTRAST).unwrap();

            let rect = curve_view_rect(cx.rect());

            for &gain in GAIN_LINES {
                let y = gain_to_y(gain, rect);

                let mut curve = Curve::default();
                curve.move_to(Point::new(rect.min.x, y));
                curve.line_to(Point::new(rect.max.x, y));

                cx.stroke(curve, 1.0, line_color);

                let mut text = TextBuffer::new(cx.fonts(), 12.0, 1.0);

                text.set_text(cx.fonts(), &format!("{:+.0} dB", gain), Default::default());

                let text_offset = Vector::new(rect.max.x + 8.0, y - 6.0);

                cx.text(&text, label_color, text_offset);
            }

            for &freq in FREQ_LINES {
                let x = freq_to_x(freq, rect);

                let mut curve = Curve::default();
                curve.move_to(Point::new(x, rect.min.y));
                curve.line_to(Point::new(x, rect.max.y));

                cx.stroke(curve, 1.0, line_color);
            }

            for &freq in FREQ_TEXT {
                let x = freq_to_x(freq, rect);

                let mut buffer = TextBuffer::new(cx.fonts(), 12.0, 1.0);

                let text = match freq >= 1000.0 {
                    true => format!("{:.0} kHz", freq / 1000.0),
                    false => format!("{:.0} Hz", freq),
                };

                buffer.set_text(cx.fonts(), &text, Default::default());

                let text_offset = Vector::new(x, rect.max.y + 12.0);

                cx.text(&buffer, label_color, text_offset);
            }

            cx.masked(rect, |cx| {
                for (i, filter) in eqo.params.filters.iter().enumerate() {
                    let mut curve = Curve::default();

                    curve.move_to(rect.center_left());

                    for i in 0..256 {
                        let frac = i as f32 / 255.0;
                        let freq = frac_to_freq(frac);

                        let gain = filter.gain_at(freq, 24000.0 + frac * 24000.0);

                        let x = freq_to_x(freq, rect);
                        let y = gain_to_y(gain, rect);

                        let point = Point::new(x, y);
                        curve.line_to(point);
                    }

                    curve.line_to(rect.center_right());

                    curve.close();

                    let color = filter_color(i, 10);

                    cx.fill(curve, FillRule::NonZero, color.fade(0.3));
                }

                let mut curve = Curve::default();

                for i in 0..512 {
                    let frac = i as f32 / 512.0;
                    let freq = frac_to_freq(frac);

                    let x = freq_to_x(freq, rect);

                    let mut gain = 0.0;

                    for filter in eqo.params.filters.iter() {
                        gain += filter.gain_at(freq, 24000.0 + frac * 24000.0);
                    }

                    let y = gain_to_y(gain, rect);

                    let point = Point::new(x, y);

                    if i == 0 {
                        curve.move_to(point);
                    } else {
                        curve.line_to(point);
                    }
                }

                cx.stroke(curve, 2.0, contrast_color);

                for (i, filter) in eqo.params.filters.iter_mut().enumerate() {
                    let center = filter_center(filter, rect);
                    let color = filter_color(i, 10);

                    cx.fill(
                        Curve::circle(center, CONTROL_RADIUS),
                        FillRule::NonZero,
                        color,
                    );
                }
            });

            cx.stroke(Curve::rect(rect), 1.0, line_color);
        });

        on_event(
            view,
            |cx, (state, eqo): &mut (CurveView, Freeq), event| match event {
                Event::PointerPressed(e) => {
                    let local = cx.local(e.position);
                    let rect = curve_view_rect(cx.rect());

                    let mut selected = None;

                    for (i, filter) in eqo.params.filters.iter().enumerate() {
                        let center = filter_center(filter, rect);

                        if center.distance(local) < CONTROL_RADIUS {
                            selected = Some(i);
                            break;
                        }
                    }

                    let Some(selected) = selected else {
                        return false;
                    };

                    match e.button {
                        PointerButton::Primary => {
                            state.selected = Some(selected);
                            true
                        }
                        PointerButton::Secondary => {
                            eqo.params.filters[selected] = Filter::new(selected as u32, 10);

                            cx.rebuild();
                            cx.draw();

                            true
                        }
                        _ => false,
                    }
                }
                Event::PointerMoved(e) => {
                    let local = cx.local(e.position);
                    let rect = curve_view_rect(cx.rect());

                    if let Some(selected) = state.selected {
                        let filter = &mut eqo.params.filters[selected];

                        *filter.freq = x_to_freq(local.x, rect);
                        *filter.gain = y_to_gain(local.y, rect);

                        *filter.freq = filter.freq.clamp(Filter::FREQ_MIN, Filter::FREQ_MAX);
                        *filter.gain = filter.gain.clamp(Filter::GAIN_MIN, Filter::GAIN_MAX);

                        if !filter.kind.uses_gain() {
                            *filter.gain = 0.0;
                        }

                        cx.rebuild();
                        cx.draw();
                    }

                    false
                }
                Event::PointerReleased(_) => state.selected.take().is_some(),
                Event::PointerScrolled(e) => {
                    let local = cx.local(e.position);
                    let rect = curve_view_rect(cx.rect());

                    let mut selected = None;

                    for (i, filter) in eqo.params.filters.iter().enumerate() {
                        let center = filter_center(filter, rect);

                        if center.distance(local) < CONTROL_RADIUS {
                            selected = Some(i);
                            break;
                        }
                    }

                    if let Some(selected) = selected {
                        let filter = &mut eqo.params.filters[selected];

                        *filter.q += e.delta.y * 0.1 * *filter.q;
                        *filter.q = filter.q.clamp(Filter::Q_MIN, Filter::Q_MAX);

                        cx.rebuild();
                        cx.draw();
                    }

                    false
                }
                _ => false,
            },
        )
    })
}

fn curve_view_rect(rect: Rect) -> Rect {
    Rect::new(
        rect.min + Vector::all(12.0),
        rect.max - Vector::new(50.0, 30.0),
    )
}

fn filter_color(index: usize, max: usize) -> Color {
    let hue = index as f32 / max as f32;

    Color::okhsl(hue * 360.0, 0.8, 0.8)
}

fn freq_to_x(freq: f32, rect: Rect) -> f32 {
    let factor = f32::log2(Filter::FREQ_MAX / Filter::FREQ_MIN);
    (f32::log2(freq) - Filter::FREQ_MIN.log2()) / factor * rect.width() + rect.min.x
}

fn frac_to_freq(frac: f32) -> f32 {
    let factor = f32::log2(Filter::FREQ_MAX / Filter::FREQ_MIN);
    f32::powf(2.0, frac * factor + Filter::FREQ_MIN.log2())
}

fn x_to_freq(x: f32, rect: Rect) -> f32 {
    let frac = (x - rect.min.x) / rect.width();
    frac_to_freq(frac)
}

fn gain_to_y(gain: f32, rect: Rect) -> f32 {
    let span = Filter::GAIN_MAX - Filter::GAIN_MIN;
    (1.0 - (gain - Filter::GAIN_MIN) / span) * rect.height() + rect.min.y
}

fn y_to_gain(y: f32, rect: Rect) -> f32 {
    let span = Filter::GAIN_MAX - Filter::GAIN_MIN;
    -Filter::GAIN_MIN - (y - rect.min.y) / rect.height() * span
}

fn filter_center(filter: &Filter, rect: Rect) -> Point {
    let x = freq_to_x(*filter.freq, rect);
    let y = gain_to_y(*filter.gain, rect);

    Point::new(x, y)
}
