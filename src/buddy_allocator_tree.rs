use bit_field::BitField;
use std::cell::Cell;
use std::cmp::{Ord, PartialOrd, Eq, PartialEq, Ordering};
use array_init;
use intrusive_collections::{RBTreeLink, RBTree, KeyAdapter, SinglyLinkedList, SinglyLinkedListLink};
use intrusive_collections::rbtree::CursorMut;
use super::{MIN_ORDER, MAX_ORDER, ORDERS, top_level_blocks};

#[derive(Debug)]
pub struct Block {
    link: RBTreeLink,
    bit_field: Cell<u64>,
}

impl Block {
    fn new(begin_address: usize, order: u8, used: bool) -> Self {
        let mut bit_field = 0u64;
        bit_field.set_bit(0, used);
        bit_field.set_bits(1..8, order as u64);
        bit_field.set_bits(8..64, begin_address as u64);

        Block {
            link: RBTreeLink::new(),
            bit_field: Cell::new(bit_field),
        }
    }

    fn used(&self) -> bool {
        self.bit_field.get().get_bit(0)
    }

    /// Set the state of this block. Unsafe because the caller could not have unique access to the
    /// block. Needed to mutate the block while it is in the tree
    unsafe fn set_used(&self, used: bool) {
        let mut copy = self.bit_field.get();
        copy.set_bit(0, used);

        self.bit_field.set(copy)
    }

    fn order(&self) -> u8 {
        self.bit_field.get().get_bits(1..8) as u8 // 7 bits for max = 64
    }

    fn address(&self) -> usize {
        self.bit_field.get().get_bits(8..64) as usize // max physical memory = 2^56 - 1 bytes
    }
}

intrusive_adapter!(BlockAdapter = Box<Block>: Block { link: RBTreeLink });

impl<'a> KeyAdapter<'a> for BlockAdapter {
    type Key = usize;
    fn get_key(&self, block: &'a Block) -> usize {
        block.address()
    }
}

impl PartialOrd for Block {
    fn partial_cmp(&self, other: &Block) -> Option<Ordering> {
        self.address().partial_cmp(&other.address())
    }
}

impl Ord for Block {
    fn cmp(&self, other: &Block) -> Ordering {
        self.address().cmp(&other.address())
    }
}

impl PartialEq for Block {
    fn eq(&self, other: &Block) -> bool {
        let properties_eq = self.order() == other.order() && self.used() == other.used();
        let address_eq = self.address() == other.address();

        // Addresses can't be the same without properties being the same
        if cfg!(debug_assertions) && address_eq == true && !properties_eq {
            panic!("Addresses can't be the same without properties being the same!");
        }

        properties_eq && address_eq
    }
}

impl Eq for Block {}

#[derive(Debug)]
pub struct BuddyAllocator<L: FreeList> {
    tree: RBTree<BlockAdapter>,
    free: [L; ORDERS as usize],
}

pub trait FreeList {
    fn push(&mut self, addr: usize);
    fn pop(&mut self) -> Option<usize>;
    /// Search for an address and remove it from the list
    fn remove(&mut self, addr: usize) -> Option<()>;
}

impl FreeList for Vec<usize> {
    fn push(&mut self, addr: usize) {
        Vec::push(self, addr);
    }

    fn pop(&mut self) -> Option<usize> {
        Vec::pop(self)
    }

    fn remove(&mut self, addr: usize) -> Option<()> {
        self.remove(self.iter().position(|i| *i == addr)?);
        Some(())
    }
}

#[derive(Debug)]
struct Address {
    link: SinglyLinkedListLink,
    address: usize,
}

impl Address {
    /// Creates a new, unlinked [Address].
    fn new(address: usize) -> Address {
        Address {
            link: SinglyLinkedListLink::new(),
            address,
        }
    }
}

intrusive_adapter!(AddressAdapter = Box<Address>: Address { link: SinglyLinkedListLink });

impl FreeList for SinglyLinkedList<AddressAdapter> {
    fn push(&mut self, addr: usize) {
        self.push_front(Box::new(Address::new(addr)))
    }

    fn pop(&mut self) -> Option<usize> {
        self.pop_front().map(|b| b.address)
    }

    fn remove(&mut self, addr: usize) -> Option<()> {
        let pos = self.iter().position(|i| i.address == addr)?;

        let mut cursor = self.front_mut();

        // Get cursor to be elem before position
        if pos > 0 {
            for _ in 0..pos - 1 {
                cursor.move_next();
            }
        }

        cursor.remove_next().unwrap();

        Some(())
    }
}

impl BuddyAllocator<Vec<usize>> {
    fn new() -> Self {
        BuddyAllocator {
            tree: RBTree::new(BlockAdapter::new()),
            free: array_init::array_init(|_| Vec::new()),
        }
    }
}

impl BuddyAllocator<SinglyLinkedList<AddressAdapter>> {
    fn new() -> Self {
        BuddyAllocator {
            tree: RBTree::new(BlockAdapter::new()),
            free: array_init::array_init(|_| SinglyLinkedList::new(AddressAdapter::new())),
        }
    }
}

impl<L: FreeList> BuddyAllocator<L> {
    fn create_top_level(&mut self, begin_address: usize) -> CursorMut<BlockAdapter> {
        self.free[MAX_ORDER as usize].push(begin_address);
        self.tree.insert(Box::new(
            Block::new(begin_address, MAX_ORDER, false),
        ))
    }

    /// Splits a block in place, returning the addresses of the two blocks split. Does not add them
    /// to the free list, or remove the original.
    ///
    /// # Panicking
    ///
    /// 1. Index incorrect and points null block (this is a programming error)
    /// 2. Attempt to split used block (this is also a programming error)
    fn split(cursor: &mut CursorMut<BlockAdapter>) -> Result<[usize; 2], BlockSplitError> {
        let block = cursor.get().unwrap();

        if block.used() == true {
            panic!("Attempted to split used block {:?}!", block);
        }

        let original_order = block.order();
        let order = original_order - 1;

        if block.order() == 0 {
            return Err(BlockSplitError::BlockSmallestPossible);
        }

        let buddies: [Block; 2] = array_init::array_init(|n| {
            let block = Block::new(
                if n == 0 {
                    block.address()
                } else {
                    block.address() + 2usize.pow((order + MIN_ORDER) as u32)
                },
                order,
                false,
            );

            block
        });

        let [first, second] = buddies;
        let addrs = [first.address(), second.address()];

        cursor.replace_with(Box::new(first)).unwrap();
        cursor.insert_after(Box::new(second));

        Ok(addrs)
    }


    /// Find a frame of a given order or splits other frames recursively until one is made and then
    /// returns a cursor pointing to it. Does not set state to used.
    ///
    /// # Panicking
    ///
    /// Panics if the order is greater than max or if a programming error is encountered such as
    /// attempting to split a block of the smallest possible size.
    fn find_or_split<'a>(
        free: &mut [L; 19],
        tree: &'a mut RBTree<BlockAdapter>,
        order: u8,
    ) -> Result<CursorMut<'a, BlockAdapter>, BlockAllocateError> {
        if order > MAX_ORDER {
            panic!("Order {} larger than max of {}!", order, MAX_ORDER);
        }

        let next_free = free[order as usize].pop();

        match next_free {
            Some(addr) => Ok(tree.find_mut(&addr)),
            None if order == MAX_ORDER => Err(BlockAllocateError::NoBlocksAvailable),
            None => {
                let mut cursor = BuddyAllocator::find_or_split(free, tree, order + 1)?;
                debug_assert!(!cursor.is_null(), "Find or split must return a valid pointer!");


                // Split block and remove it from the free list
                let old_address = cursor.get().unwrap().address();
                let addresses = Self::split(&mut cursor).unwrap();
                free[order as usize + 1].remove(old_address);

                // Push split blocks to free list
                free[order as usize].push(addresses[0]);
                free[order as usize].push(addresses[1]);

                Ok(cursor)
            }
        }
    }

    fn allocate_exact(&mut self, order: u8) -> Result<CursorMut<BlockAdapter>, BlockAllocateError> {
        if order > MAX_ORDER {
            return Err(BlockAllocateError::OrderTooLarge(order));
        }

        let block = BuddyAllocator::find_or_split(&mut self.free, &mut self.tree, order)?;

        // Safe because we have exclusive access to `block`.
        unsafe {
            block.get().unwrap().set_used(true);
        }

        let address = block.get().unwrap().address();
        self.free[order as usize].remove(address);

        Ok(block)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum BlockSplitError {
    BlockSmallestPossible,
}

#[derive(Debug, Copy, Clone)]
pub enum BlockAllocateError {
    NoBlocksAvailable,
    OrderTooLarge(u8),
}

pub fn demo_vecs(print_addresses: bool, blocks: u32, block_size: u8) {
    let allocator = BuddyAllocator::<Vec<usize>>::new();
    demo(allocator, print_addresses, blocks, block_size)
}

pub fn demo_linked_lists(print_addresses: bool, blocks: u32, block_size: u8) {
    let allocator = BuddyAllocator::<SinglyLinkedList<AddressAdapter>>::new();
    demo(allocator, print_addresses, blocks, block_size)
}


fn demo<'a, L: FreeList>(
    mut allocator: BuddyAllocator<L>,
    print_addresses: bool,
    blocks: u32,
    block_size: u8,
) {
    let top_level_blocks = top_level_blocks(blocks, block_size);

    for block_number in 0..top_level_blocks {
        allocator.create_top_level(
            2usize.pow((MAX_ORDER + MIN_ORDER) as u32) * block_number as usize,
        );
    }

    for _ in 0..blocks {
        let cursor = allocator.allocate_exact(block_size).unwrap();
        let addr = cursor.get().unwrap().address();

        if print_addresses {
            println!("Address: {:#x}", addr);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_create_top_level() {
        let mut allocator = BuddyAllocator::<Vec<usize>>::new();
        allocator.create_top_level(0);
        allocator.create_top_level(2usize.pow((MIN_ORDER + MAX_ORDER) as u32));

        let expected = vec![
            Block::new(0, MAX_ORDER, false),
            Block::new(
                2usize.pow((MIN_ORDER + MAX_ORDER) as u32),
                MAX_ORDER,
                false,
            ),
        ];

        assert_eq!(
            allocator.tree.into_iter().map(|b| *b).collect::<Vec<Block>>(),
            expected
        );
    }

    #[test]
    fn split() {
        let mut allocator = BuddyAllocator::<Vec<usize>>::new();
        let mut block = allocator.create_top_level(0);
        BuddyAllocator::<Vec<usize>>::split(&mut block).unwrap();

        let expected = vec![
            Block::new(0, MAX_ORDER - 1, false),
            Block::new(
                2usize.pow((MIN_ORDER + MAX_ORDER - 1) as u32),
                MAX_ORDER - 1,
                false,
            ),
        ];

        assert_eq!(
            allocator.tree.into_iter().map(|b| *b).collect::<Vec<Block>>(),
            expected
        );
    }

    #[test]
    fn test_allocate_exact_with_free() {
        let mut allocator = BuddyAllocator::<Vec<usize>>::new();
        allocator.create_top_level(0);
        let cursor = allocator.allocate_exact(MAX_ORDER).unwrap();
        let expected_block = Block::new(0, MAX_ORDER, true);
        assert_eq!(*cursor.get().unwrap(), expected_block);
    }

    #[test]
    fn test_allocate_exact_no_free() {
        let mut allocator = BuddyAllocator::<Vec<usize>>::new();
        allocator.create_top_level(0);
        let cursor = allocator.allocate_exact(MAX_ORDER - 2).unwrap();
        let expected_block = Block::new(0, MAX_ORDER - 2, true);

        assert_eq!(*cursor.get().unwrap(), expected_block);
    }

    #[test]
    fn test_linked_list_remove() {
        let mut list = SinglyLinkedList::<AddressAdapter>::new(AddressAdapter::new());
        list.push_front(Box::new(Address::new(1)));
        list.push_front(Box::new(Address::new(2)));
        list.push_front(Box::new(Address::new(3)));
        list.push_front(Box::new(Address::new(4)));
        list.push_front(Box::new(Address::new(5)));
        list.remove(2).unwrap();

        assert_eq!(list.iter().map(|i| i.address).collect::<Vec<usize>>(), vec![5, 4, 3, 1]);
    }

    #[test]
    fn test_unique_addresses_vecs() {
        let mut allocator = BuddyAllocator::<Vec<usize>>::new();

        for block_number in 0..top_level_blocks(1000, 0) {
            allocator.create_top_level(
                2usize.pow((MAX_ORDER + MIN_ORDER) as u32) * block_number as usize,
            );
        }

        let mut seen = Vec::with_capacity(1000);
        for _ in 0..1000 {
            let cursor = allocator.allocate_exact(0).unwrap();
            let addr = cursor.get().unwrap().address();

            if seen.contains(&addr) {
                panic!("Allocator must return addresses never been allocated before!");
            } else {
                seen.push(addr);
            }
        }
    }

    #[test]
    fn test_unique_addresses_linked_lists() {
        let mut allocator = BuddyAllocator::<SinglyLinkedList<AddressAdapter>>::new();

        for block_number in 0..top_level_blocks(1000, 0) {
            allocator.create_top_level(
                2usize.pow((MAX_ORDER + MIN_ORDER) as u32) * block_number as usize,
            );
        }

        let mut seen = Vec::with_capacity(1000);
        for _ in 0..1000 {
            let cursor = allocator.allocate_exact(0).unwrap();
            let addr = cursor.get().unwrap().address();

            if seen.contains(&addr) {
                panic!("Allocator must return addresses never been allocated before!");
            } else {
                seen.push(addr);
            }
        }
    }


    #[test]
    fn test_block_bitfields() {
        let block = Block::new(2usize.pow(56) - 1,64, false);

        assert!(!block.used());
        assert_eq!(block.order(), 64);
        assert_eq!(block.address(), 2usize.pow(56) - 1);

        unsafe { block.set_used(true) };
        assert!(block.used());
    }
}
