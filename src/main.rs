#![feature(nll)]
#![feature(slice_patterns)]

extern crate array_init;

mod buddy_allocator_lists;

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
    // Demo the lists allocator
    buddy_allocator_lists::demo_vec();

}
