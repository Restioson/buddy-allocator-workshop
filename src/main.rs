#![feature(const_fn)]
#![feature(nll)]
#![feature(slice_patterns)]

/// Core facade so it will be easy to port to no_std
// TODO do this for alloc & linkedlist too
mod core {
    pub use std::*;
}

mod buddy_allocator;

extern crate spin;
#[macro_use]
extern crate lazy_static;
extern crate array_init;

use buddy_allocator::*;

use spin::Mutex;

lazy_static! {
    static ref BUDDY_ALLOCATOR: Mutex<BuddyAllocator> = Mutex::new(BuddyAllocator::new());
}

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
    fn get_power_of_two(self) -> u8 {
        use self::PageSize::*;
        match self {
            Kib4 => 12,
            Mib2 => 21,
            Gib1 => 30,
        }
    }
}

fn main() {
    BUDDY_ALLOCATOR.lock().create_top_level(0);
    for _ in 0..10 {
        let addr = BUDDY_ALLOCATOR.lock().alloc(PageSize::Kib4) as usize;
        println!("Address: {:#x}", addr);
    }
}
