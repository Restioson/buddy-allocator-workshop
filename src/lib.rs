#![feature(const_fn)]
#![feature(nll)]
#![feature(slice_patterns)]
#![feature(duration_extras)]
#![feature(arbitrary_self_types)]
#![feature(test)]
#![feature(integer_atomics)]
#![feature(box_syntax)]
#![feature(custom_attribute)]
#![feature(plugin)]
#![cfg_attr(feature = "flame_profile", plugin(flamer))]

extern crate array_init;
extern crate test;
#[macro_use]
extern crate static_assertions;
#[macro_use]
extern crate intrusive_collections;
extern crate bit_field;
#[cfg(feature = "flame_profile")]
extern crate flame;

pub mod buddy_allocator_bitmap;
pub mod buddy_allocator_lists;
pub mod buddy_allocator_tree;

/// Number of orders. **This constant is OK to modify for configuration.**
pub const LEVEL_COUNT: u8 = 19;
/// The maximum order. **This constant is not Ok to modify for configuration.**
pub const MAX_ORDER: u8 = LEVEL_COUNT - 1;
/// The minimum order. All orders are in context of this -- i.e the size of a block of order `k` is
/// `2^(k + MIN_ORDER)`, not `2^k`. **This constant is OK to modify for configuration.**
///
/// # Note
///
/// **NB: Must be greater than log base 2 of 4096.** This is so that 4kib pages can always be
/// allocated, regardless of min order.
pub const BASE_ORDER: u8 = 12;
const_assert!(__min_order_less_or_eq_than_4kib; BASE_ORDER <= 12);
/// The size as a power of two of the maximum order.
pub const MAX_ORDER_SIZE: u8 = BASE_ORDER + MAX_ORDER;

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
    pub fn power_of_two(self) -> u8 {
        use self::PageSize::*;
        match self {
            Kib4 => 12,
            Mib2 => 21,
            Gib1 => 30,
        }
    }
}

pub fn top_level_blocks(blocks: u32, block_size: u8) -> u64 {
    let a = 2f64.powi(i32::from(block_size + BASE_ORDER)) * f64::from(blocks)
        / 2f64.powi(i32::from(MAX_ORDER + BASE_ORDER));

    a.ceil() as u64
}
