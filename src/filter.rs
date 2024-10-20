use std::f32::consts::PI;

use num::Complex;
use ori_vst::prelude::*;

#[derive(Params)]
pub struct Filter {
    pub freq: Float,
    pub gain: Float,
    pub q: Float,
    pub kind: FilterKind,
}

impl Filter {
    pub const FREQ_MIN: f32 = 20.0;
    pub const FREQ_MAX: f32 = 20000.0;
    pub const GAIN_MIN: f32 = -18.0;
    pub const GAIN_MAX: f32 = 18.0;
    pub const Q_MIN: f32 = 0.1;
    pub const Q_MAX: f32 = 10.0;

    pub fn new(index: u32, count: u32) -> Filter {
        // distribute the default frequency over the range
        // 20 Hz to 20 kHz, remembering that the space is logarithmic

        let frac = (index as f32 + 0.5) / count as f32;
        let factor = f32::log2(Self::FREQ_MAX / Self::FREQ_MIN);
        let freq = f32::powf(2.0, frac * factor + Self::FREQ_MIN.log2());

        let default_q = if index == 0 || index == count - 1 {
            0.5
        } else {
            2.0
        };

        let kind = match index {
            0 => FilterKind::LowShelf,
            _ if index == count - 1 => FilterKind::HighShelf,
            _ => FilterKind::Peak,
        };

        Filter {
            freq: Float::new(freq, Self::FREQ_MIN..=Self::FREQ_MAX)
                .with_name(format!("Frequency ({})", index))
                .with_automate(),

            gain: Float::new(0.0, Self::GAIN_MIN..=Self::GAIN_MAX)
                .with_name(format!("Gain ({})", index))
                .with_automate(),

            q: Float::new(default_q, Self::Q_MIN..=Self::Q_MAX)
                .with_name(format!("Q ({})", index))
                .with_automate(),

            kind,
        }
    }

    pub fn gain_at(&self, freq: f32, sample_rate: f32) -> f32 {
        let mut state = FilterState::default();
        state.set_params(self, sample_rate);
        state.gain_at(freq, sample_rate)
    }
}

#[derive(Default)]
pub struct FilterState {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a0: f32,
    pub a1: f32,
    pub a2: f32,
    pub z1: f32,
    pub z2: f32,
    pub y1: f32,
    pub y2: f32,
}

impl FilterState {
    pub fn set_params(&mut self, filter: &Filter, sample_rate: f32) {
        let freq = *filter.freq;
        let gain = *filter.gain;
        let q = *filter.q;

        self.set_params_inner(freq, gain, q, filter.kind, sample_rate);
    }

    fn set_params_inner(
        &mut self,
        freq: f32,
        gain: f32,
        q: f32,
        kind: FilterKind,
        sample_rate: f32,
    ) {
        let a = f32::powf(10.0, gain / 40.0);

        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();

        match kind {
            FilterKind::LowPass => {
                let w0 = f32::tan(w0 / 2.0);

                self.b0 = w0;
                self.b1 = w0;
                self.b2 = 0.0;
                self.a0 = w0 + 1.0;
                self.a1 = w0 - 1.0;
                self.a2 = 0.0;
            }
            FilterKind::LowPass2 => {
                let alpha = w0.sin() / (2.0 * q);

                self.b0 = (1.0 - cos_w0) / 2.0;
                self.b1 = 1.0 - cos_w0;
                self.b2 = (1.0 - cos_w0) / 2.0;
                self.a0 = 1.0 + alpha;
                self.a1 = -2.0 * cos_w0;
                self.a2 = 1.0 - alpha;
            }
            FilterKind::LowShelf => {
                let alpha = w0.sin() / 2.0 * (1.0 / q);

                self.b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha);
                self.b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
                self.b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha);
                self.a0 = (a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha;
                self.a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
                self.a2 = (a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha;
            }
            FilterKind::HighPass => {
                let w0 = f32::tan(w0 / 2.0);

                self.b0 = 1.0;
                self.b1 = -1.0;
                self.b2 = 0.0;
                self.a0 = w0 + 1.0;
                self.a1 = w0 - 1.0;
                self.a2 = 0.0;
            }
            FilterKind::HighPass2 => {
                let alpha = w0.sin() / (2.0 * q);

                self.b0 = (1.0 + cos_w0) / 2.0;
                self.b1 = -(1.0 + cos_w0);
                self.b2 = (1.0 + cos_w0) / 2.0;
                self.a0 = 1.0 + alpha;
                self.a1 = -2.0 * cos_w0;
                self.a2 = 1.0 - alpha;
            }
            FilterKind::HighShelf => {
                let alpha = w0.sin() / 2.0 * (1.0 / q);

                self.b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha);
                self.b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
                self.b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha);
                self.a0 = (a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha;
                self.a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
                self.a2 = (a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha;
            }
            FilterKind::Peak => {
                let alpha = f32::sin(w0) / (2.0 * q);

                self.b0 = 1.0 + alpha * a;
                self.b1 = -2.0 * cos_w0;
                self.b2 = 1.0 - alpha * a;
                self.a0 = 1.0 + alpha / a;
                self.a1 = -2.0 * cos_w0;
                self.a2 = 1.0 - alpha / a;
            }
            FilterKind::Notch => {
                let alpha = f32::sin(w0) / (2.0 * q);

                self.b0 = 1.0;
                self.b1 = -2.0 * cos_w0;
                self.b2 = 1.0;
                self.a0 = 1.0 + alpha;
                self.a1 = -2.0 * cos_w0;
                self.a2 = 1.0 - alpha;
            }
        }

        self.b0 /= self.a0;
        self.b1 /= self.a0;
        self.b2 /= self.a0;
        self.a1 /= self.a0;
        self.a2 /= self.a0;
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        let b0 = self.b0;
        let b1 = self.b1;
        let b2 = self.b2;
        let a1 = self.a1;
        let a2 = self.a2;
        let z1 = self.z1;
        let z2 = self.z2;
        let y1 = self.y1;
        let y2 = self.y2;

        let out = b0 * sample + b1 * z1 + b2 * z2 - a1 * y1 - a2 * y2;

        self.z2 = z1;
        self.z1 = sample;
        self.y2 = y1;
        self.y1 = out;

        out
    }

    pub fn gain_at(&self, freq: f32, sample_rate: f32) -> f32 {
        let a0 = self.a0;

        let b0 = self.b0 * a0;
        let b1 = self.b1 * a0;
        let b2 = self.b2 * a0;
        let a1 = self.a1 * a0;
        let a2 = self.a2 * a0;

        let w = 2.0 * PI * freq / sample_rate;

        let num = b0 + b1 * Complex::new(0.0, -w).exp() + b2 * Complex::new(0.0, -2.0 * w).exp();
        let den = a0 + a1 * Complex::new(0.0, -w).exp() + a2 * Complex::new(0.0, -2.0 * w).exp();
        let h = num / den;

        20.0 * f32::log10(h.norm())
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FilterKind {
    LowPass,
    LowPass2,
    LowShelf,
    HighPass,
    HighPass2,
    HighShelf,
    Peak,
    Notch,
}

impl FilterKind {
    pub const MAX_ID: u32 = 7;

    pub fn abbreviation(&self) -> &str {
        match self {
            FilterKind::LowPass => "LP",
            FilterKind::LowPass2 => "LP2",
            FilterKind::LowShelf => "LS",
            FilterKind::HighPass => "HP",
            FilterKind::HighPass2 => "HP2",
            FilterKind::HighShelf => "HS",
            FilterKind::Peak => "PK",
            FilterKind::Notch => "NT",
        }
    }

    pub fn uses_gain(&self) -> bool {
        match self {
            FilterKind::LowPass => false,
            FilterKind::LowPass2 => false,
            FilterKind::LowShelf => true,
            FilterKind::HighPass => false,
            FilterKind::HighPass2 => false,
            FilterKind::HighShelf => true,
            FilterKind::Peak => true,
            FilterKind::Notch => false,
        }
    }

    pub fn id(&self) -> u32 {
        match self {
            FilterKind::LowPass => 0,
            FilterKind::LowPass2 => 1,
            FilterKind::LowShelf => 2,
            FilterKind::HighPass => 3,
            FilterKind::HighPass2 => 4,
            FilterKind::HighShelf => 5,
            FilterKind::Peak => 6,
            FilterKind::Notch => 7,
        }
    }

    pub fn from_id(id: u32) -> Option<FilterKind> {
        match id {
            0 => Some(FilterKind::LowPass),
            1 => Some(FilterKind::LowPass2),
            2 => Some(FilterKind::LowShelf),
            3 => Some(FilterKind::HighPass),
            4 => Some(FilterKind::HighPass2),
            5 => Some(FilterKind::HighShelf),
            6 => Some(FilterKind::Peak),
            7 => Some(FilterKind::Notch),
            _ => None,
        }
    }

    pub fn prev(&self) -> FilterKind {
        let id = self.id();
        let prev_id = (id + Self::MAX_ID - 1) % Self::MAX_ID;
        FilterKind::from_id(prev_id).unwrap()
    }

    pub fn next(&self) -> FilterKind {
        let id = self.id();
        let next_id = (id + 1) % Self::MAX_ID;
        FilterKind::from_id(next_id).unwrap()
    }
}

impl Param for FilterKind {
    fn get(&self) -> f32 {
        self.id() as f32
    }

    fn set(&mut self, plain: f32) {
        *self = FilterKind::from_id(plain.round() as u32).unwrap_or(FilterKind::Peak);
    }

    fn default(&self) -> f32 {
        Self::Peak.get()
    }

    fn plain(&self, normalized: f32) -> f32 {
        normalized * Self::MAX_ID as f32
    }

    fn normalize(&self, plain: f32) -> f32 {
        plain / Self::MAX_ID as f32
    }

    fn unit(&self) -> Unit {
        Unit::Custom(String::new())
    }

    fn steps(&self) -> Option<i32> {
        Some(Self::MAX_ID as i32)
    }

    fn flags(&self) -> ParamFlags {
        ParamFlags::LIST
    }

    fn to_string(&self, plain: f32) -> String {
        match FilterKind::from_id(plain.round() as u32) {
            Some(kind) => match kind {
                FilterKind::LowPass => String::from("Low Pass"),
                FilterKind::LowPass2 => String::from("Low Pass 2"),
                FilterKind::LowShelf => String::from("Low Shelf"),
                FilterKind::HighPass => String::from("High Pass"),
                FilterKind::HighPass2 => String::from("High Pass 2"),
                FilterKind::HighShelf => String::from("High Shelf"),
                FilterKind::Peak => String::from("Peak"),
                FilterKind::Notch => String::from("Notch"),
            },
            None => {
                println!("FilterKind::to_string: invalid id: {}", plain);
                String::from("Peak")
            }
        }
    }

    fn from_string(&self, string: &str) -> f32 {
        match string {
            "Low Pass" => FilterKind::LowPass.get(),
            "Low Pass 2" => FilterKind::LowPass2.get(),
            "Low Shelf" => FilterKind::LowShelf.get(),
            "High Pass" => FilterKind::HighPass.get(),
            "High Pass 2" => FilterKind::HighPass2.get(),
            "High Shelf" => FilterKind::HighShelf.get(),
            "Peak" => FilterKind::Peak.get(),
            "Notch" => FilterKind::Notch.get(),
            _ => FilterKind::Peak.get(),
        }
    }
}
