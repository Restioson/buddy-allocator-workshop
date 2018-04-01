#![feature(plugin)]
#![feature(const_fn)]
#![feature(nll)]
#![feature(slice_patterns)]
#![feature(custom_attribute)]
#![feature(duration_extras)]
#![feature(arbitrary_self_types)]

#![plugin(phf_macros)]
#![cfg_attr(feature="flame_profile", feature(plugin, custom_attribute))]
#![cfg_attr(feature="flame_profile", plugin(flamer))]

#![allow(unused_attributes)]

#[macro_use]
extern crate structopt;
extern crate array_init;
extern crate phf;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate static_assertions;
#[macro_use]
extern crate intrusive_collections;
extern crate bit_field;
#[cfg(feature="flame_profile")]
extern crate flame;

mod buddy_allocator_lists;
mod buddy_allocator_tree;

use std::time::Instant;
use structopt::StructOpt;
use failure::Fail;

/// Number of orders. **This constant is OK to modify for configuration.**
const ORDERS: u8 = 19;
/// The maximum order. **This constant is not Ok to modify for configuration.**
const MAX_ORDER: u8 = ORDERS - 1;
/// The minimum order. All orders are in context of this -- i.e the size of a block of order `k` is
/// `2^(k + MIN_ORDER)`, not `2^k`. **This constant is OK to modify for configuration.**
///
/// # Note
///
/// **NB: Must be greater than log base 2 of 4096.** This is so that 4kib pages can always be
/// allocated, regardless of min order.
const MIN_ORDER: u8 = 12;
const_assert!(__min_order_less_or_eq_than_4kib; MIN_ORDER <= 12);

#[rustfmt_skip] // Puts phf_map! with same indentation level as the key => value
static DEMOS: phf::Map<&'static str, fn(bool, u32, u8)> = phf_map! {
    "linked_lists" => buddy_allocator_lists::demo_linked_lists,
    "vecs" => buddy_allocator_lists::demo_vecs,
    "rb_tree_vecs" => buddy_allocator_tree::demo_vecs,
    "rb_tree_linked_lists" => buddy_allocator_tree::demo_linked_lists,
};

const DEFAULT_DEMOS: &[&str] = &[
    "vecs",
    "linked_lists",
    "rb_tree_vecs",
    "rb_tree_linked_lists",
];

trait PhysicalAllocator {
    fn alloc(&mut self, size: PageSize) -> *const u8;
    fn dealloc(&mut self, addr: *const u8);
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PageSize {
    Kib4,
    Mib2,
    Gib1,
}

impl PageSize {
    fn power_of_two(self) -> u8 {
        use self::PageSize::*;
        match self {
            Kib4 => 12,
            Mib2 => 21,
            Gib1 => 30,
        }
    }
}

pub fn top_level_blocks(blocks: u32, block_size: u8) -> u64 {
    let a = 2f64.powi(i32::from(block_size + MIN_ORDER)) * f64::from(blocks) /
        2f64.powi(i32::from(MAX_ORDER + MIN_ORDER));

    a.ceil() as u64
}

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
                DEMOS
                    .get(&*name)
                    .ok_or(DemosError::UnknownDemo { name: name.to_string() })
                    .raise(),
                name,
            )
        })
        .collect::<Vec<_>>() // Force detect unknown demos ASAP
        .into_iter()
        .for_each(|(demo, name)| {
            run_demo(*demo, print_addresses, blocks, order, name)
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

    println!("Running {} demo...", name);
    let begin = Instant::now();
    demo(print_addresses, blocks, order);
    let time_taken = Instant::now().duration_since(begin);
    println!(
        "Finished {} demo in {}s",
        name.replace('_', " "),
        time_taken.as_secs() as f64 + f64::from(time_taken.subsec_nanos()) / NANOS_PER_SEC,
    );
}

#[cfg(feature = "flame_profile")]
fn flame_dump() {
    use std::fs::File;
    flame::dump_html(&mut File::create("flame-graph.html").unwrap()).unwrap();
}

#[cfg(not(feature = "flame_profile"))]
fn flame_dump() {}