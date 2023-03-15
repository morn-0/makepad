#![feature(portable_simd)]

pub use makepad_widgets;
pub use makepad_widgets::makepad_draw;
pub use makepad_widgets::makepad_platform;
pub mod app;
mod mandelbrot;
mod mandelbrot_simd;