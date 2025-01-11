#![allow(dead_code)]

use crate::style_text;
use color_eyre::owo_colors::OwoColorize;
use std::{fmt::Debug, time::Duration};

pub fn print_indexes_arg(indexes: &Vec<usize>) {
    println!("only downloading indexes: {:?}.", indexes);
}

pub fn downloading_panel_data_msg(i: usize, max: usize) -> String {
    format!(
        "{}",
        format_args!("({} .. {})", style_text!(i + 1, url), style_text!(max, url))
    )
}

pub fn fetching_img_bytes() -> String {
    style_text!("fetching img bytes")
}

pub fn indexes_failed_msg(len: usize) -> String {
    format!(
        "{}: {} {}",
        style_text!("WARNING", severe),
        style_text!(len, bold),
        style_text!("panels failed; trying once", error),
    )
}

pub fn print_elapsed(elapsed: Duration) {
    let elap = format!("{:.?}", elapsed);
    println!("{}\n", style_text!(elap, url));
}

pub fn print_reqerr_count(count: usize, title: impl Debug) {
    let title = format!("{:?}", title);
    let msg = format!(
        "{}: {} {}: {}.",
        style_text!("\nWARNING", severe),
        style_text!(count, bold),
        style_text!("error(s) occured; failed on", error),
        style_text!(title, bold)
    );
    println!("{msg}");
}
