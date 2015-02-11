use std::cmp::{max, min, partial_min};
use std::num::Float;

use super::config;

type ColorTransformerConfig = config::Transform;
type HSVTransformer = config::HSV;

// % is remainder, need modulo
fn modulo(l: f32, r: f32) -> f32 {
	if l >= 0.0 {
		l % r
	} else {
		r + l % r
	}
}

#[derive(Clone)]
pub struct RgbTransformer<'a> {
	pub r: &'a config::ColorSettings,
	pub g: &'a config::ColorSettings,
	pub b: &'a config::ColorSettings
}

pub trait Pixel {
	fn to_rgb(&self) -> RGB8;
	fn to_hsv(&self) -> HSV;

	fn rgb_transform(&self, rgb_transformer: &RgbTransformer) -> RGB8 {
		let rgb = self.to_rgb();
		let mut colors = [rgb.r, rgb.g, rgb.b];
		let transformers = [
			&rgb_transformer.r,
			&rgb_transformer.g,
			&rgb_transformer.b];

		for (color, transformer) in colors.iter_mut().zip(transformers.iter()) {
			let c = (*color as f32 / 255.0).powf(transformer.gamma)
				* transformer.whitelevel
				* (1.0 - transformer.blacklevel) + transformer.blacklevel;
			*color = (if c >= transformer.threshold { c } else { 0.0 }
				* 255.0) as u8;
		}
		RGB8{r: colors[0], g: colors[1], b: colors[2]}
	}

	fn hsv_transform(&self, transformer: &HSVTransformer) -> HSV {
		let hsv = self.to_hsv();
		HSV{hue: hsv.hue,
			saturation: partial_min(1.0, hsv.saturation * transformer.saturationGain)
				.unwrap_or(1.0),
			value: partial_min(1.0, hsv.value * transformer.valueGain).unwrap_or(1.0)}
	}
}

#[derive(Clone, Debug)]
pub struct RGB8 {
	pub r: u8,
	pub g: u8,
	pub b: u8
}
impl Pixel for RGB8 {
	fn to_rgb(&self) -> RGB8 {
		self.clone()
	}

	fn to_hsv(&self) -> HSV {
		let max = max(max(self.r, self.g), self.b);
		let min = min(min(self.r, self.g), self.b);
		let chroma = max - min;

		let hue = 1.0/6.0 * if chroma == 0 {
			0.0
		} else if max == self.r {
			modulo((self.g as f32 - self.b as f32) / chroma as f32, 6.0)
		} else if max == self.g {
			((self.b as f32 - self.r as f32) / chroma as f32) + 2.0
		} else {
			((self.r as f32 - self.g as f32) / chroma as f32) + 4.0
		};

		let value = max;

		let saturation = if value == 0 {
			0.0
		} else {
			chroma as f32 / value as f32
		};

		HSV{hue: hue, saturation: saturation, value: value as f32 / 255.0}
	}
}

#[derive(Clone, Debug)]
pub struct HSV {
	pub hue: f32,
	pub saturation: f32,
	pub value: f32
}
impl Pixel for HSV {
	fn to_rgb(&self) -> RGB8 {
		if self.saturation == 0.0 {
			let v = (self.value * 255.0) as u8;
			RGB8{r: v, g: v, b: v}
		} else {
			let sector_f = self.hue * 6.0;
			let sector = sector_f as u8;
			let factorial_part = sector_f - sector as f32;
			let val_255 = self.value * 255.0;
			let v_8bit = val_255 as u8;

			let p = (val_255 * (1.0 - self.saturation)) as u8;
			let q = (val_255 * (1.0 - self.saturation * factorial_part)) as u8;
			let t = (val_255 * (1.0 - self.saturation * (1.0 - factorial_part))) as u8;
			
			let (r, g, b) = match sector {
				0 => (v_8bit, t, p),
				1 => (q, v_8bit, p),
				2 => (p, v_8bit, t),
				3 => (p, q, v_8bit),
				4 => (t, p, v_8bit),
				_ => (v_8bit, p, q),
			};
			RGB8{r: r, g: g, b: b}
		}
	}

	fn to_hsv(&self) -> HSV {
		self.clone()
	}
}