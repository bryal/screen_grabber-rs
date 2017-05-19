// The MIT License (MIT)
//
// Copyright (c) 2015 Johan Johansson
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

// TODO: TESTS!
// TODO: Color shift
// TODO: Support capturing a virtual desktop, multiple monotors together
// TODO: Optional solid color or animation when there's no signal

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate clock_ticks;
extern crate serial;
extern crate captrs;
extern crate simd;

#[cfg(feature = "cpuprofiler")]
extern crate cpuprofiler;

use captrs::Capturer;
use capture::ImageAnalyzer;
use color::{Rgb8, RgbTransformer, HSVTransformer, Color};
use config::parse_led_indices;
use serial::SerialPort;
use std::{thread, time};
use std::cmp::{max, Ordering};

mod config;
mod color;
mod capture;

type SmoothFn = fn(Rgb8, Rgb8, f32) -> Rgb8;

/// Returns smallest of `a` and `b` if there is one, else returns `expect`
fn partial_min<T: PartialOrd>(a: T, b: T, expect: T) -> T {
    match a.partial_cmp(&b) {
        Some(Ordering::Less) |
        Some(Ordering::Equal) => a,
        Some(Ordering::Greater) => b,
        None => expect,
    }
}
/// Returns greatest of `a` and `b` if there is one, else returns `expect`
fn partial_max<T: PartialOrd>(a: T, b: T, expect: T) -> T {
    match a.partial_cmp(&b) {
        Some(Ordering::Greater) |
        Some(Ordering::Equal) => a,
        Some(Ordering::Less) => b,
        None => expect,
    }
}

/// A special header is expected by the corresponding LED streaming code running on the Arduino.
/// This only needs to be initialized once since the number of LEDs remains constant.
fn new_pixel_buf_header(n_leds: u16) -> [u8; 6] {
    // In the two below, not sure why the -1 in `(n_leds - 1)` is needed,
    // but that's how LEDstream on the Arduino expects it
    let count_high = ((n_leds - 1) >> 8) as u8; // LED count high byte
    let count_low = ((n_leds - 1) & 0xff) as u8; // LED count low byte

    ['A' as u8,
     'd' as u8,
     'a' as u8,
     count_high,
     count_low,
     count_high ^ count_low ^ 0x55 /* Checksum */]
}

/// A timer to track time passed between refreshes. Can for example be used to limit frame rate.
struct FrameTimer {
    before: f64,
    last_frame_dt: f64,
}
impl FrameTimer {
    fn new() -> FrameTimer {
        FrameTimer {
            before: clock_ticks::precise_time_s(),
            last_frame_dt: 0.0,
        }
    }

    /// For how long the previous frame lasted
    fn last_frame_dt(&self) -> f64 {
        self.last_frame_dt
    }

    /// Time passed since last tick
    fn dt_to_now(&mut self) -> f64 {
        let now = clock_ticks::precise_time_s();
        let dt = now - self.before;
        if dt >= 0.0 {
            dt
        } else {
            self.before = now;
            0.0
        }
    }

    /// An update/frame/refresh has occured; take the time.
    fn tick(&mut self) {
        let now = clock_ticks::precise_time_s();
        self.last_frame_dt = partial_max(now - self.before, 0.0, 0.0);
        self.before = now;
    }
}

/// Writes color data via serial to LEDstream compatible device
struct ColorWriter {
    con: serial::SystemPort,
    header: [u8; 6],
}

impl ColorWriter {
    /// Configure serial writing given a serial port, baud rate, and header to write before
    /// each data write
    fn new(port: &str, baud_rate: u32, header: [u8; 6]) -> Self {
        let mut serial_con = serial::open(port).unwrap();

        let baud_rate = serial::BaudRate::from_speed(baud_rate as usize);

        serial_con.reconfigure(&|cfg| cfg.set_baud_rate(baud_rate)).unwrap();

        ColorWriter { con: serial_con, header: header }
    }

    /// Write a buffer of color data to the LEDstream device
    fn write_colors(&mut self, color_data: &[Rgb8]) {
        use std::io::Write;

        let color_bytes = color::rgbs_as_bytes(color_data);

        match self.con.write(&self.header) {
            Ok(hn) if hn == self.header.len() => {
                match self.con.write(color_bytes) {
                    Ok(bn) if bn == color_bytes.len() => (),
                    Ok(bn) => {
                        println!("Failed to write all bytes of RGB data. Wrote {} of {}",
                                 bn,
                                 color_data.len())
                    }
                    Err(e) => println!("Failed to write RGB data, {}", e),
                }
            }
            Ok(_) => println!("Failed to write all bytes in header"),
            Err(e) => println!("Failed to write header, {}", e),
        }
    }
}

/// Update the colors to output by analyzing the captured frame for each led
fn update_out_color_data(out_pixels: &mut [Rgb8],
                         frame_analyzer: &ImageAnalyzer,
                         leds: &[config::Region],
                         leds_transformers: &[Vec<(Option<RgbTransformer>,
                                                   Option<&HSVTransformer>)>],
                         smooth: SmoothFn,
                         smooth_factor: f32) {
    for (i, &led) in leds.iter().enumerate() {
        let avg_color = frame_analyzer.average_color(led);

        let to_pixel = leds_transformers[i]
            .iter()
            .map(|&(ref opt_rgb, ref opt_hsv)| (opt_rgb.as_ref(), opt_hsv.as_ref()))
            .fold(Color::RGB(avg_color),
                  |acc_color, transformers| match transformers {
                      (Some(rgb_tr), Some(hsv_tr)) => {
                          Color::HSV(hsv_tr.transform(rgb_tr.transform(acc_color.into_rgb())
                                                            .to_hsv()))
                      }
                      (Some(rgb_tr), _) => Color::RGB(rgb_tr.transform(acc_color.into_rgb())),
                      (_, Some(hsv_tr)) => Color::HSV(hsv_tr.transform(acc_color.into_hsv())),
                      _ => acc_color,
                  })
            .into_rgb();

        out_pixels[i] = smooth(out_pixels[i], to_pixel, smooth_factor);
    }
}

/// Specifies how to loop over the main body
#[cfg(not(feature = "cpuprofiler"))]
fn main_loop<F: FnMut() -> bool>(mut body: F) {
    loop {
        if !body() {
            break;
        }
    }
}

/// Loop over the main body a fixed number of times and track performance with a profiler
#[cfg(feature = "cpuprofiler")]
fn main_loop<F: FnMut() -> bool>(mut body: F) {
    cpuprofiler::PROFILER.lock().unwrap().start("./prof.profile").unwrap();

    for _ in 0..1000 {
        if !body() {
            break;
        }
    }

    cpuprofiler::PROFILER.lock().unwrap().stop().unwrap();
}

fn main() {
    let config = config::parse_config();

    let leds: &[_] = &config.leds;

    let mut led_transformers_list: Vec<_> = vec![Vec::with_capacity(1); leds.len()];

    // Add color transforms from config to each led in matching vec
    for transform_conf in config.color.transform.iter() {
        let hsv_transformer =
            if !transform_conf.hsv.is_default() { Some(&transform_conf.hsv) } else { None };

        let rgb_transformer = if !(transform_conf.red.is_default() &&
                                   transform_conf.green.is_default() &&
                                   transform_conf.red.is_default()) {
            Some(RgbTransformer {
                r: transform_conf.red.clone(),
                g: transform_conf.green.clone(),
                b: transform_conf.blue.clone(),
            })
        } else {
            None
        };

        for range in parse_led_indices(&transform_conf.leds, leds.len()).iter() {
            for transformers in led_transformers_list[range.clone()].iter_mut() {
                transformers.push((rgb_transformer.clone(), hsv_transformer));
            }
        }
    }

    // Header to write before led data
    let out_header = new_pixel_buf_header(leds.len() as u16);

    let mut color_writer = ColorWriter::new(&config.device.output, config.device.rate, out_header);

    // Skeleton for the output led pixel buffer to write to arduino
    let mut out_pixels = vec![Rgb8 { r: 0, g: 0, b: 0 }; leds.len()];

    let mut capturer = Capturer::new(0).unwrap();

    let capture_frame_interval = 1.0 / config.framegrabber.frequency_Hz;

    // Function to use when smoothing led colors
    let smooth = match config.color.smoothing.type_.as_ref() {
        "linear" => color::linear_smooth as SmoothFn,
        _ => color::no_smooth as SmoothFn,
    };

    // max w/ 1 to avoid future divide by zero
    let smooth_time_const = max(config.color.smoothing.time_ms, 1) as f64 / 1000.0;

    let led_refresh_interval = 1.0 / config.color.smoothing.update_frequency;

    println!("Helion - An LED streamer\nNumber of LEDs: {}\nResize resolution: {} x {}\nCapture \
              rate: {} fps\nLED refresh rate: {} hz\nSerial port: {}",
             leds.len(),
             config.framegrabber.width,
             config.framegrabber.height,
             config.framegrabber.frequency_Hz,
             1.0 / led_refresh_interval,
             config.device.output);

    let mut capture_timer = FrameTimer::new();
    let mut led_refresh_timer = FrameTimer::new();

    main_loop(|| {
        led_refresh_timer.tick();

        // Don't capture new frame if going faster than frame limit,
        // but still proceed to smooth leds
        if capture_timer.dt_to_now() > capture_frame_interval {
            // If something goes wrong, last frame is reused

            if let Err(e) = capturer.capture_store_frame() {
                println!("Error: {:?}", e);
                thread::sleep(time::Duration::from_millis(1_000));
                capturer = Capturer::new(0).unwrap();
                return true;
            }

            capture_timer.tick();
        }

        if let Some(frame) = capturer.get_stored_frame() {
            let (w, h) = capturer.geometry();
            let frame_analyzer = ImageAnalyzer::new(frame,
                                                    w as usize,
                                                    h as usize,
                                                    config.framegrabber.width,
                                                    config.framegrabber.height);


            let smooth_factor = (led_refresh_timer.last_frame_dt() / smooth_time_const) as f32;

            update_out_color_data(&mut out_pixels,
                                  &frame_analyzer,
                                  leds,
                                  &led_transformers_list,
                                  smooth,
                                  smooth_factor);

            color_writer.write_colors(&out_pixels)
        }

        let time_left = led_refresh_interval - led_refresh_timer.dt_to_now();
        if time_left > 0.0 {
            let ms = if time_left > 0.0 { time_left * 1_000.0 } else { 0.0 };

            thread::sleep(time::Duration::from_millis(ms as u64));
        }
        true
    })
}
