//! MP3無音検出ミニ（Rust実験版）
//!
//! Delphi実験版（Free Pascal・生 Win32）と 1:1 で比べるための Rust 版。
//! 仕様・文言・ffmpeg の呼び出し方は Delphi 版に一致させてある。
//! 違うのはタイトルが「Rust実験版」になることだけ。
#![cfg_attr(windows, windows_subsystem = "windows")]

mod parse;

#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Mp3SilenceMini-Rust は Windows 専用の実験アプリです。");
}
