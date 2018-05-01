#![feature(custom_attribute)]
#![feature(plugin)]
#![feature(nll)]
#![cfg_attr(feature = "flame_profile", plugin(flamer))]
#![allow(unused_attributes)]

extern crate buddy_allocator_workshop;

#[macro_use]
extern crate structopt;
#[cfg(feature = "flame_profile")]
extern crate flame;
#[macro_use]
extern crate failure;

use buddy_allocator_workshop::*;
use failure::Fail;
use std::time::Instant;
use structopt::StructOpt;

const DEFAULT_DEMOS: &[&str] = &[
    "vecs",
    "linked_lists",
    "rb_tree_vecs",
    "rb_tree_linked_lists",
    "bitmap",
];

#[derive(StructOpt, Debug)]
#[structopt(name = "buddy_allocator_workshop")]
struct Options {
    /// Print the addresses of blocks as they are allocated. This will slow down performance, and as
    /// such should not be used for benchmarking.
    #[structopt(short = "p", long = "print-addresses")]
    print_addresses: bool,
    /// Which demos to run. Defaults to all demos. Accepted values: `vecs`, `linked_lists`,
    /// `rb_tree_vecs`, `rb_tree_linked_lists`.
    #[structopt(short = "d", long = "demos")]
    demos: Vec<String>,
    /// How many blocks to demo allocate. Defaults to 100 000
    #[structopt(short = "b", long = "blocks")]
    blocks: Option<u32>,
    /// The order of the blocks to allocate. Defaults to `0`, which is `2^MIN_ORDER` bytes. Must not
    /// be greater than `MAX_ORDER`.
    #[structopt(short = "o", long = "order")]
    order: Option<u8>,
}

#[derive(Debug, Fail)]
enum DemosError {
    #[fail(display = "Unknown demo \"{}\"", name)]
    UnknownDemo { name: String },
    #[fail(display = "Order {} too large, max is {}", order, max_order)]
    OrderTooLarge {
        order: u8,
        /// Must be equal to [MAX_ORDER]. Required as a field due to a limitation in fail.
        max_order: u8,
    },
}

fn main() {
    let Options {
        print_addresses,
        demos,
        blocks,
        order,
    } = Options::from_args();

    let demos = if demos.is_empty() {
        DEFAULT_DEMOS.iter().map(|s| s.to_string()).collect()
    } else {
        demos
    };

    let (blocks, order) = (
        blocks.unwrap_or(100_000),
        order.unwrap_or(PageSize::Kib4.power_of_two() - MIN_ORDER),
    );

    if order > MAX_ORDER {
        raise(DemosError::OrderTooLarge {
            order,
            max_order: MAX_ORDER,
        });
    }

    demos
        .into_iter()
        .map(|name| {
            (
                match &*name {
                    "linked_lists" => buddy_allocator_lists::demo_linked_lists,
                    "vecs" => buddy_allocator_lists::demo_vecs,
                    "rb_tree_vecs" => buddy_allocator_tree::demo_vecs,
                    "rb_tree_linked_lists" => buddy_allocator_tree::demo_linked_lists,
                    "bitmap" => buddy_allocator_bitmap::demo,
                    _ => Err(DemosError::UnknownDemo { name: name.to_string() }).raise(),
                },
                name
            )
        })
        .collect::<Vec<_>>() // Force detect unknown demos ASAP
        .into_iter()
        .for_each(|(demo, name)| {
            run_demo(demo, print_addresses, blocks, order, name)
        });

    flame_dump();
}

trait ResultExt<T> {
    fn raise(self) -> T;
}

impl<T, E: Fail> ResultExt<T> for Result<T, E> {
    fn raise(self) -> T {
        match self {
            Ok(ok) => ok,
            Err(err) => raise(err),
        }
    }
}

fn raise<F: Fail>(failure: F) -> ! {
    println!("error: {}", failure);
    std::process::exit(1)
}

fn run_demo(demo: fn(bool, u32, u8), print_addresses: bool, blocks: u32, order: u8, name: String) {
    const NANOS_PER_SEC: f64 = 1_000_000_000.0; // Taken from std::time::Duration because las
    const RUN_COUNT: usize = 8;

    println!("Running {} demo...", name);
    let begin = Instant::now();
    for _ in 0..RUN_COUNT {
        demo(print_addresses, blocks, order);
    }
    let time_taken = Instant::now().duration_since(begin);
    println!(
        "Finished {} demo in {}s",
        name.replace('_', " "),
        (time_taken.as_secs() as f64 + f64::from(time_taken.subsec_nanos()) / NANOS_PER_SEC) / RUN_COUNT as f64,
    );
}

#[cfg(feature = "flame_profile")]
fn flame_dump() {
    use std::fs::File;
    flame::dump_html(&mut File::create("flame-graph.html").unwrap()).unwrap();
}

#[cfg(not(feature = "flame_profile"))]
fn flame_dump() {}
