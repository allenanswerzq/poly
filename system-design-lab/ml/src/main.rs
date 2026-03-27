#![allow(
    dead_code,
    unused_variables,
    unused_imports,
    clippy::needless_range_loop,
    clippy::all
)]
//! # ML Interview Problem Set
//!
//! All problems implemented from scratch in Rust — no framework, pure math.
//! Covers: fundamentals, attention, architecture, training, inference, advanced.
//!
//! Run: cargo run -p ml-problems

mod advanced;
mod architecture;
mod attention;
mod fundamentals;
mod inference;
mod tensor;
mod training;

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║        ML Interview Problem Set                  ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    println!("━━━ Fundamentals ━━━");
    fundamentals::demo();

    println!("━━━ Attention Mechanisms ━━━");
    attention::demo();

    println!("━━━ Architecture & Adaptation ━━━");
    architecture::demo();

    println!("━━━ Training & Optimization ━━━");
    training::demo();

    println!("━━━ Inference & Decoding ━━━");
    inference::demo();

    println!("━━━ Advanced ━━━");
    advanced::demo();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              All {} problems complete!             ║", 40);
    println!("╚══════════════════════════════════════════════════╝");
}
