use std::{fmt::Display, str::FromStr};

pub struct ColorParseError;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    // Named colors

    /// #000000FF
    pub fn black() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    }

    /// #FF0000FF
    pub fn red() -> Self {
        Self {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }
    }

    /// #00FF00FF
    pub fn green() -> Self {
        Self {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        }
    }

    /// #FFFF00FF
    pub fn yellow() -> Self {
        Self {
            r: 255,
            g: 255,
            b: 0,
            a: 255,
        }
    }

    /// #0000FFFF
    pub fn blue() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        }
    }

    /// #8000FFFF
    pub fn purple() -> Self {
        Self {
            r: 128,
            g: 0,
            b: 255,
            a: 255,
        }
    }

    /// #00FFFFFF
    pub fn cyan() -> Self {
        Self {
            r: 0,
            g: 255,
            b: 255,
            a: 255,
        }
    }

    /// #FFFFFFFF
    pub fn white() -> Self {
        Self {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }
    }

    /// #FF00FFFF
    pub fn magenta() -> Self {
        Self {
            r: 255,
            g: 0,
            b: 255,
            a: 255,
        }
    }

    /// Converts a hex string to a Color
    /// Parses a hex color string in either RRGGBB or RRGGBBAA format
    pub fn hex(s: &str) -> Result<Self, ColorParseError> {
        let mut len = 0;
        let val = s
            .chars()
            .skip_while(|c| *c == '#')
            .map(|v| {
                len += 1;
                v
            })
            .map(parse_hexdigit)
            .fold(Ok(0_u64), |acc, v| Ok(v? as u64 + (acc?) * 16))?;

        // Extract the individual channels
        let (r, g, b, a) = match len {
            // RRGGBB
            6 => ((val & 0xFF0000) >> 16, (val & 0xFF00) >> 8, val & 0xFF, 255),
            // RRGGBBAA
            8 => (
                (val & 0xFF000000) >> 24,
                (val & 0xFF0000) >> 16,
                (val & 0xFF00) >> 8,
                val & 0x0FF,
            ),
            // Other
            _ => return Err(ColorParseError),
        };

        dbg!(r, g, b, a);

        Ok(Self {
            r: r as _,
            g: g as _,
            b: b as _,
            a: a as _,
        })
    }

    /// Constructs a fully opaque color with rgb components
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Constructs a color from rgba values
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Constructs a color from HSL values
    /// `h`: [0.0, 360.0]
    /// `s`: [0.0, 1.0]
    /// `l`: [0.0, 1.0]
    pub fn hsl(h: f32, s: f32, l: f32) -> Self {
        Self::hsla(h, s, l, 1.0)
    }

    /// Constructs a color from HSL and alpha values
    /// `h`: [0.0, 360.0]
    /// `s`: [0.0, 1.0]
    /// `l`: [0.0, 1.0]
    /// `a`: [0.0, 1.0]
    pub fn hsla(h: f32, s: f32, l: f32, a: f32) -> Self {
        if s == 0.0 {
            // Achromatic, i.e., grey.
            let l = percent_to_byte(l);
            return Self {
                r: l,
                g: l,
                b: l,
                a: percent_to_byte(a),
            };
        }

        let h = h / 360.0; // treat this as 0..1 instead of degrees
        let s = s;
        let l = l;

        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - (l * s)
        };
        let p = 2.0 * l - q;

        Self {
            r: percent_to_byte(hue_to_rgb(p, q, h + 1.0 / 3.0)),
            b: percent_to_byte(hue_to_rgb(p, q, h)),
            g: percent_to_byte(hue_to_rgb(p, q, h - 1.0 / 3.0)),
            a: percent_to_byte(a),
        }
    }

    pub fn to_hsla(&self) -> (f32, f32, f32, f32) {
        use std::cmp::{max, min};

        let mut h: f32;
        let s: f32;
        let l: f32;

        let max = max(max(self.r, self.g), self.b);
        let min = min(min(self.r, self.g), self.b);

        // Normalized RGB: Divide everything by 255 to get percentages of colors.
        let (r, g, b) = (
            self.r as f32 / 255_f32,
            self.g as f32 / 255_f32,
            self.b as f32 / 255_f32,
        );
        let (min, max) = (min as f32 / 255_f32, max as f32 / 255_f32);

        // Luminosity is the average of the max and min rgb color intensities.
        l = (max + min) / 2.0;

        // Saturation
        let delta: f32 = max - min;
        if delta == 0.0 {
            // it's gray
            return (0.0, 0.0, 1.0, byte_to_percent(self.a));
        }

        // it's not gray
        if l < 0.5_f32 {
            s = delta / (max + min);
        } else {
            s = delta / (2_f32 - max - min);
        }

        // Hue
        let r2 = (((max - r) / 6_f32) + (delta / 2_f32)) / delta;
        let g2 = (((max - g) / 6_f32) + (delta / 2_f32)) / delta;
        let b2 = (((max - b) / 6_f32) + (delta / 2_f32)) / delta;

        h = match max {
            x if x == r => b2 - g2,
            x if x == g => (1_f32 / 3_f32) + r2 - b2,
            _ => (2_f32 / 3_f32) + g2 - r2,
        };

        // Fix wraparounds
        if h < 0 as f32 {
            h += 1_f32;
        } else if h > 1 as f32 {
            h -= 1_f32;
        }

        // Hue is precise to milli-degrees, e.g. `74.52deg`.
        let h_degrees = (h * 360_f32 * 100_f32).round() / 100_f32;

        (h_degrees, s, l, byte_to_percent(self.a))
    }

    pub fn to_array(&self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn to_array_f32(&self) -> [f32; 4] {
        [
            byte_to_percent(self.r),
            byte_to_percent(self.g),
            byte_to_percent(self.b),
            byte_to_percent(self.a),
        ]
    }
}

impl Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "#{:02x}{:02x}{:02x}{:02x}",
            self.r, self.g, self.b, self.a
        )
    }
}

impl FromStr for Color {
    type Err = ColorParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::hex(s)
    }
}

// Helper functions
fn byte_to_percent(a: u8) -> f32 {
    (a as f32) / 255.0
}

fn percent_to_byte(percent: f32) -> u8 {
    (percent * 255.0).round() as u8
}

// Convert Hue to RGB Ratio
//
// From <https://github.com/jariz/vibrant.js/> by Jari Zwarts
fn hue_to_rgb(p: f32, q: f32, t: f32) -> f32 {
    // Normalize
    let t = if t < 0.0 {
        t + 1.0
    } else if t > 1.0 {
        t - 1.0
    } else {
        t
    };

    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 1.0 / 2.0 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

// Parses a single char into hex
fn parse_hexdigit(digit: char) -> Result<u8, ColorParseError> {
    let digit = digit.to_ascii_lowercase();
    if digit >= '0' && digit <= '9' {
        return Ok(digit as u8 - '0' as u8);
    } else if digit >= 'a' && digit <= 'f' {
        return Ok(digit as u8 - 'a' as u8 + 10);
    } else {
        Err(ColorParseError)
    }
}
