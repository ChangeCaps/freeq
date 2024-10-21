use std::{
    f32::consts::PI,
    sync::Arc,
    time::{Duration, Instant},
};

use filter::{Filter, FilterState};
use num::Complex;
use ori_vst::prelude::*;
use realfft::{RealFftPlanner, RealToComplex};

mod filter;

#[derive(Params)]
pub struct FreeqParams {
    #[param(group)]
    filters: [Filter; 10],
}

vst3!(Freeq);

pub struct Freeq {
    params: FreeqParams,
    filters: [[FilterState; 10]; 2],
    fft: Arc<dyn RealToComplex<f32>>,
    prev_input: f32,
    prev_output: f32,
    buffer_a: Vec<f32>,
    buffer_b: Vec<f32>,
    complex: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    spectrum: Vec<f32>,
    current: usize,
    sample_rate: f32,
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
        Window::new().title("FreeQ").size(640, 500).resizable(false)
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
            fft: RealFftPlanner::new().plan_fft_forward(Self::FFT_SIZE),
            prev_input: 0.0,
            prev_output: 0.0,
            buffer_a: vec![0.0; Self::FFT_SIZE],
            buffer_b: vec![0.0; Self::FFT_SIZE],
            complex: vec![Complex::new(0.0, 0.0); Self::FFT_SIZE / 2 + 1],
            scratch: vec![Complex::new(0.0, 0.0); Self::FFT_SIZE / 2 + 1],
            spectrum: vec![0.0; Self::FFT_SIZE / 2 + 1],
            current: 0,
            sample_rate: 44100.0,
        }
    }

    fn params(&mut self) -> &mut dyn Params {
        &mut self.params
    }

    fn ui(&mut self) -> impl View<Self> + 'static {
        let mut filters = Vec::new();

        for i in 0..self.params.filters.len() {
            let filter = filter_options(self, i);
            filters.push(filter);
        }

        let filters = hstack(filters);

        vstack![flex(curve_view(self)), filters].align(Align::Start)
    }

    fn activate(&mut self, _audio_layout: &AudioLayout, buffer_layout: &BufferLayout) -> Activate {
        self.sample_rate = buffer_layout.sample_rate;

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
            let mut average = 0.0;

            for (sample, filters) in samples.zip(self.filters.iter_mut()) {
                for filter in filters.iter_mut() {
                    if !filter.enabled {
                        continue;
                    }

                    *sample = filter.process(*sample);
                }

                average += *sample;
            }

            average /= buffer.channels() as f32;

            // apply high-pass filter to remove DC offset
            let high_pass = 0.999 * (self.prev_output + average - self.prev_input);
            self.prev_input = average;
            self.prev_output = high_pass;

            let i_a = self.current;
            let i_b = (self.current + self.buffer_a.len() / 2) % self.buffer_a.len();

            self.buffer_a[i_a] = high_pass;
            self.buffer_b[i_b] = high_pass;
            self.current += 1;
            self.current %= self.buffer_a.len();

            if self.current == 0 {
                self.compute_fft(false);
            } else if self.current == self.buffer_a.len() / 2 {
                self.compute_fft(true);
            }
        }

        Process::Done
    }
}

impl Freeq {
    pub const FFT_SIZE: usize = 4096;

    fn hann_window(i: usize, n: usize) -> f32 {
        0.5 * (1.0 - f32::cos(2.0 * PI * i as f32 / (n - 1) as f32))
    }

    pub fn compute_fft(&mut self, is_b: bool) {
        let buffer = if is_b {
            &mut self.buffer_b
        } else {
            &mut self.buffer_a
        };

        // apply window function
        for (i, sample) in buffer.iter_mut().enumerate() {
            *sample *= Self::hann_window(i, Self::FFT_SIZE);
        }

        self.fft
            .process_with_scratch(buffer, &mut self.complex, &mut self.scratch)
            .unwrap();

        let norm = f32::powf(2.0, 2.0 / Self::FFT_SIZE as f32);

        for (spectrum, complex) in self.spectrum.iter_mut().zip(self.complex.iter()) {
            let magnitude = complex.norm() * norm;
            let magnitude = magnitude.max(1.0e-6);

            *spectrum *= 0.6;
            *spectrum += magnitude * 0.4;
        }
    }

    fn spectrum_x(&self, i: usize, rect: Rect) -> f32 {
        let freq = i as f32 * self.sample_rate / Self::FFT_SIZE as f32 + 1.0;
        freq_to_x(freq, rect)
    }

    fn spectrum_y(&self, i: usize, rect: Rect) -> f32 {
        let freq = i as f32 * self.sample_rate / Self::FFT_SIZE as f32 + 1.0e-3;

        let pink_noise = 3.0 * (f32::log10(freq / 20.0) / f32::log10(2.0));

        let spectrum = self.spectrum[i];

        let gain = 20.0 * f32::log10(spectrum + 1.0e-6);
        let gain = gain + pink_noise;
        let gain = gain / 80.0;

        rect.bottom() - gain * rect.height()
    }
}

const CONTROL_RADIUS: f32 = 8.0;
const SPLINE_TENSION: f32 = 0.2;

const GAIN_LINES: &[f32] = &[-18.0, -12.0, -6.0, 0.0, 6.0, 12.0, 18.0];

const FREQ_LINES: &[f32] = &[
    20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 200.0, 300.0, 400.0, 500.0, 600.0,
    700.0, 800.0, 900.0, 1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0,
    10000.0, 20000.0,
];

const FREQ_TEXT: &[f32] = &[
    20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0,
];

#[derive(Default)]
struct CurveView {
    selected: Option<usize>,
    last_click: Option<Instant>,
}

impl CurveView {
    fn is_double_click(&mut self) -> bool {
        match self.last_click {
            Some(last_click) => {
                let now = Instant::now();
                let elapsed = now.duration_since(last_click);

                if elapsed < Duration::from_millis(500) {
                    self.last_click = None;
                    true
                } else {
                    self.last_click = Some(now);
                    false
                }
            }
            None => {
                self.last_click = Some(Instant::now());
                false
            }
        }
    }
}

fn curve_view(_data: &mut Freeq) -> impl View<Freeq> {
    with_state_default(|_state, _data| {
        let view = painter(|cx, (_state, data): &mut (CurveView, Freeq)| {
            let styles = cx.styles();
            let line_color = styles.get(Theme::OUTLINE).unwrap();
            let label_color = styles.get(Theme::CONTRAST_LOW).unwrap();
            let contrast_color = styles.get(Theme::CONTRAST).unwrap();

            let rect = curve_view_rect(cx.rect());

            for (i, &gain) in GAIN_LINES.iter().enumerate() {
                let y = gain_to_y(gain, rect);

                let mut curve = Curve::default();
                curve.move_to(Point::new(rect.min.x, y));
                curve.line_to(Point::new(rect.max.x, y));

                cx.stroke(curve, 1.0, line_color);

                let mut text = TextBuffer::new(cx.fonts(), 12.0, 1.0);

                text.set_text(cx.fonts(), &format!("{:+.0} dB", gain), Default::default());

                let text_offset = match i > 0 {
                    true => Vector::new(rect.max.x + 8.0, y - 6.0),
                    false => Vector::new(rect.max.x + 8.0, y - 8.0),
                };

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

                let text_offset = Vector::new(x - 8.0, rect.max.y + 8.0);

                cx.text(&buffer, label_color, text_offset);
            }

            cx.masked(rect, |cx| {
                let mut points: Vec<Point> = Vec::with_capacity(data.spectrum.len());

                for i in 0..data.spectrum.len() {
                    let x = data.spectrum_x(i, rect);
                    let y = data.spectrum_y(i, rect);

                    let point = Point::new(x, y);

                    if let Some(last) = points.last_mut() {
                        if last.x.floor() == point.x.floor() {
                            last.y = f32::max(last.y, point.y);
                            continue;
                        }
                    }

                    points.push(point);
                }

                let mut curve = Curve::default();

                curve.move_to(rect.bottom_left());

                for i in 1..points.len() {
                    let a = match i == 1 {
                        true => points[0],
                        false => {
                            let p0 = points[i - 2];
                            let p1 = points[i - 1];
                            let p2 = points[i];

                            p1 + (p2 - p0) * SPLINE_TENSION
                        }
                    };

                    let b = match i == points.len() - 1 {
                        true => points[i],
                        false => {
                            let p0 = points[i - 1];
                            let p1 = points[i];
                            let p2 = points[i + 1];

                            p1 - (p2 - p0) * SPLINE_TENSION
                        }
                    };

                    let c = points[i];

                    curve.cubic_to(a, b, c);
                }

                curve.line_to(rect.bottom_right());
                curve.close();

                cx.fill(curve.clone(), FillRule::NonZero, contrast_color.fade(0.1));
                cx.stroke(curve, 1.0, contrast_color.fade(0.5));

                for (i, filter) in data.params.filters.iter().enumerate() {
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

                    match *filter.enabled {
                        true => cx.fill(curve, FillRule::NonZero, color.fade(0.4)),
                        false => cx.fill(curve, FillRule::NonZero, color.fade(0.3).desaturate(0.3)),
                    }
                }

                let mut curve = Curve::default();

                for i in 0..512 {
                    let frac = i as f32 / 512.0;
                    let freq = frac_to_freq(frac);

                    let x = freq_to_x(freq, rect);

                    let mut gain = 0.0;

                    for filter in data.params.filters.iter() {
                        if !*filter.enabled {
                            continue;
                        }

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

                for (i, filter) in data.params.filters.iter_mut().enumerate().rev() {
                    let center = filter_center(filter, rect);

                    let color = match *filter.enabled {
                        true => filter_color(i, 10),
                        false => filter_color(i, 10).desaturate(0.5),
                    };

                    cx.fill(
                        Curve::circle(center, CONTROL_RADIUS),
                        FillRule::NonZero,
                        color,
                    );

                    cx.fill(
                        Curve::circle(center, CONTROL_RADIUS - 2.0),
                        FillRule::NonZero,
                        color.darken(0.3).desaturate(0.2),
                    );
                }
            });

            cx.stroke(Curve::rect(rect), 1.0, line_color);
        });

        on_event(view, |cx, (state, data): &mut (CurveView, Freeq), event| {
            cx.animate();

            match event {
                Event::PointerPressed(e) => {
                    let local = cx.local(e.position);
                    let rect = curve_view_rect(cx.rect());

                    let mut selected = None;

                    for (i, filter) in data.params.filters.iter().enumerate() {
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

                            if state.is_double_click() {
                                data.params.filters[selected] = Filter::new(selected as u32, 10);

                                cx.rebuild();
                                cx.draw();

                                state.selected = None;
                            }

                            true
                        }
                        PointerButton::Secondary => {
                            let filter = &mut data.params.filters[selected];

                            *filter.enabled = !*filter.enabled;

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
                        let filter = &mut data.params.filters[selected];

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
                Event::PointerReleased(e) if e.button == PointerButton::Primary => {
                    state.selected.take().is_some()
                }
                Event::PointerScrolled(e) => {
                    let local = cx.local(e.position);
                    let rect = curve_view_rect(cx.rect());

                    let mut selected = None;

                    for (i, filter) in data.params.filters.iter().enumerate() {
                        let center = filter_center(filter, rect);

                        if center.distance(local) < CONTROL_RADIUS {
                            selected = Some(i);
                            break;
                        }
                    }

                    if let Some(selected) = selected {
                        let filter = &mut data.params.filters[selected];

                        *filter.q += e.delta.y * 0.1 * *filter.q;
                        *filter.q = filter.q.clamp(Filter::Q_MIN, Filter::Q_MAX);

                        cx.rebuild();
                        cx.draw();
                    }

                    false
                }
                Event::Animate(_) => {
                    cx.animate();
                    cx.draw();

                    true
                }
                _ => false,
            }
        })
    })
}

fn filter_options(data: &mut Freeq, index: usize) -> impl View<Freeq> {
    let filter = &mut data.params.filters[index];
    let color = match *filter.enabled {
        true => filter_color(index, 10),
        false => filter_color(index, 10).darken(0.5).desaturate(0.5),
    };

    let prev_kind = text("<").font_size(14.0);
    let prev_kind = button(prev_kind).padding(2.0).color(Theme::SURFACE);
    let prev_kind = on_click(prev_kind, move |cx, filter: &mut Filter| {
        filter.kind = filter.kind.prev();

        if !filter.kind.uses_gain() {
            *filter.gain = 0.0;
        }

        cx.rebuild();
        cx.draw();
    });

    let next_kind = text(">").font_size(14.0);
    let next_kind = button(next_kind).padding(2.0).color(Theme::SURFACE);
    let next_kind = on_click(next_kind, move |cx, filter: &mut Filter| {
        filter.kind = filter.kind.next();

        if !filter.kind.uses_gain() {
            *filter.gain = 0.0;
        }

        cx.rebuild();
        cx.draw();
    });

    let kind = text(filter.kind.abbreviation()).font_size(14.0);
    let kind = hstack![prev_kind, kind, next_kind].justify(Justify::SpaceBetween);
    let kind = width(FILL, pad([6.0, 0.0], kind));

    let freq = match *filter.freq < 1000.0 {
        true => format!("{:.0} Hz", *filter.freq),
        false => format!("{:.1} kHz", *filter.freq / 1000.0),
    };

    let freq = text(freq).font_size(14.0);

    let gain = text(format!("{:+.0} dB", *filter.gain)).font_size(14.0);

    let q = text(format!("{:.2}", *filter.q)).font_size(14.0);

    let view = vstack![kind, freq, gain, q].gap(2.0);

    let view = pad([8.0, 2.0, 2.0, 2.0], view);
    let view = container(view)
        .border_width([6.0, 0.0, 0.0, 0.0])
        .border_radius(2.0)
        .border_color(color);

    let view = width(64.0, view);

    focus(view, move |data: &mut Freeq, lens| {
        lens(&mut data.params.filters[index])
    })
}

fn curve_view_rect(rect: Rect) -> Rect {
    Rect::new(
        rect.min + Vector::all(18.0),
        rect.max - Vector::new(54.0, 30.0),
    )
}

fn filter_color(index: usize, max: usize) -> Color {
    let hue = index as f32 / max as f32;

    Color::okhsl(hue * 360.0, 0.8, 0.8)
}

fn freq_to_frac(freq: f32) -> f32 {
    let factor = f32::log2(Filter::FREQ_MAX / Filter::FREQ_MIN);
    (f32::log2(freq) - Filter::FREQ_MIN.log2()) / factor
}

fn frac_to_freq(frac: f32) -> f32 {
    let factor = f32::log2(Filter::FREQ_MAX / Filter::FREQ_MIN);
    f32::powf(2.0, frac * factor + Filter::FREQ_MIN.log2())
}

fn freq_to_x(freq: f32, rect: Rect) -> f32 {
    let frac = freq_to_frac(freq);
    rect.min.x + frac * rect.width()
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
