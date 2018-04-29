use test::{Bencher, black_box};
#[cfg(feature="flame_profile")]
use flame;
use bit_field::BitField;
use std::cell::Cell;
use std::ptr;
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
        bit_field.set_bits(1..8, u64::from(order));
        bit_field.set_bits(8..64, begin_address as u64);

        Block {
            link: RBTreeLink::new(),
            bit_field: Cell::new(bit_field),
        }
    }

    #[inline]
    fn used(&self) -> bool {
        self.bit_field.get().get_bit(0)
    }

    /// Set the state of this block. Unsafe because the caller could not have unique access to the
    /// block. Needed to mutate the block while it is in the tree
    #[inline]
    unsafe fn set_used(&self, used: bool) {
        let mut copy = self.bit_field.get();
        copy.set_bit(0, used);

        self.bit_field.set(copy)
    }

    #[inline]
    fn order(&self) -> u8 {
        self.bit_field.get().get_bits(1..8) as u8 // 7 bits for max = 64
    }

    #[inline]
    fn address(&self) -> usize {
        self.bit_field.get().get_bits(8..64) as usize // max physical memory = 2^56 - 1 bytes
    }
}

intrusive_adapter!(pub BlockAdapter = Box<Block>: Block { link: RBTreeLink });

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
        if cfg!(debug_assertions) && address_eq && !properties_eq {
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
    fn push(&mut self, block: *const Block);
    fn pop(&mut self) -> Option<*const Block>;
    /// Search for an address and remove it from the list
    fn remove(&mut self, addr: *const Block) -> Option<()>;
}

impl FreeList for Vec<*const Block> {
    fn push(&mut self, block: *const Block) {
        Vec::push(self, block);
    }

    fn pop(&mut self) -> Option<*const Block> {
        Vec::pop(self)
    }

    fn remove(&mut self, block: *const Block) -> Option<()> {
        self.remove(self.iter().position(|i| ptr::eq(*i, block))?);
        Some(())
    }
}

#[derive(Debug)]
pub struct BlockPtr {
    link: SinglyLinkedListLink,
    ptr: *const Block,
}

impl BlockPtr {
    /// Creates a new, unlinked [BlockPtrAdapter].
    fn new(ptr: *const Block) -> BlockPtr {
        BlockPtr {
            link: SinglyLinkedListLink::new(),
            ptr
        }
    }
}

intrusive_adapter!(pub BlockPtrAdapter = Box<BlockPtr>: BlockPtr { link: SinglyLinkedListLink });

impl FreeList for SinglyLinkedList<BlockPtrAdapter> {
    fn push(&mut self, block: *const Block) {
        self.push_front(Box::new(BlockPtr::new(block)))
    }

    fn pop(&mut self) -> Option<*const Block> {
        self.pop_front().map(|b| b.ptr)
    }

    fn remove(&mut self, block: *const Block) -> Option<()> {
        let pos = self.iter().position(|i| ptr::eq(i.ptr, block))?;

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

impl BuddyAllocator<Vec<*const Block>> {
    pub fn new() -> Self {
        BuddyAllocator {
            tree: RBTree::new(BlockAdapter::new()),
            free: array_init::array_init(|_| Vec::new()),
        }
    }
}

impl BuddyAllocator<SinglyLinkedList<BlockPtrAdapter>> {
    pub fn new() -> Self {
        BuddyAllocator {
            tree: RBTree::new(BlockAdapter::new()),
            free: array_init::array_init(|_| SinglyLinkedList::new(BlockPtrAdapter::new())),
        }
    }
}

impl<L: FreeList> BuddyAllocator<L> {
    pub fn create_top_level(&mut self, begin_address: usize) -> CursorMut<BlockAdapter> {
        let cursor = self.tree.insert(Box::new(
            Block::new(begin_address, MAX_ORDER, false),
        ));
        self.free[MAX_ORDER as usize].push(cursor.get().unwrap() as *const _);
        cursor
    }

    /// Splits a block in place, returning the addresses of the two blocks split. Does not add them
    /// to the free list, or remove the original. The cursor will point to the first block.
    ///
    /// # Panicking
    ///
    /// 1. Index incorrect and points null block (this is a programming error)
    /// 2. Attempt to split used block (this is also a programming error)
    #[cfg_attr(feature="flame_profile", flame)]
    fn split(cursor: &mut CursorMut<BlockAdapter>) -> Result<[*const Block; 2], BlockSplitError> {
        #[cfg(feature="flame_profile")]
            flame::note("split", None);
        let block = cursor.get().unwrap();

        if block.used() {
            panic!("Attempted to split used block {:?}!", block);
        }

        let original_order = block.order();
        let order = original_order - 1;

        if block.order() == 0 {
            return Err(BlockSplitError::BlockSmallestPossible);
        }

        let buddies: [Block; 2] = array_init::array_init(|n| {
            Block::new(
                if n == 0 {
                    block.address()
                } else {
                    block.address() + 2usize.pow(u32::from(order + MIN_ORDER))
                },
                order,
                false,
            )
        });

        let [first, second] = buddies;

        // Reuse the old box
        let mut old = cursor.remove().unwrap();
        *old = first;
        cursor.insert_before(old);
        cursor.insert_before(Box::new(second));

        // Reversed pointers
        let ptrs: [*const _; 2] = array_init::array_init(|_| {
            cursor.move_prev();
            cursor.get().unwrap() as *const _
        });

        Ok([ptrs[1], ptrs[0]])
    }


    /// Find a frame of a given order or splits other frames recursively until one is made and then
    /// returns a cursor pointing to it. Does not set state to used.
    ///
    /// # Panicking
    ///
    /// Panics if the order is greater than max or if a programming error is encountered such as
    /// attempting to split a block of the smallest possible size.
    #[cfg_attr(feature="flame_profile", flame)]
    fn find_or_split<'a>(
        free: &mut [L; 19],
        tree: &'a mut RBTree<BlockAdapter>,
        order: u8,
    ) -> Result<CursorMut<'a, BlockAdapter>, BlockAllocateError> {
        #[cfg(feature="flame_profile")]
        flame::note("find_or_split", None);

        if order > MAX_ORDER {
            panic!("Order {} larger than max of {}!", order, MAX_ORDER);
        }

        // Find free block of size >= order
        let next_free = free[order as usize].pop();

        match next_free {
            Some(ptr) => Ok(unsafe { tree.cursor_mut_from_ptr(ptr) }),
            None if order == MAX_ORDER => Err(BlockAllocateError::NoBlocksAvailable),
            None => {
                let mut cursor = BuddyAllocator::find_or_split(free, tree, order + 1)?;
                debug_assert!(!cursor.is_null(), "Find or split must return a valid pointer!");

                // Split block and remove it from the free list
                let old_ptr = cursor.get().unwrap() as *const _;
                let ptrs = Self::split(&mut cursor).unwrap();
                free[order as usize + 1].remove(old_ptr);

                // Push split blocks to free list
                free[order as usize].push(ptrs[0]);
                free[order as usize].push(ptrs[1]);

                Ok(cursor)
            }
        }
    }

    #[cfg_attr(feature="flame_profile", flame)]
    pub fn allocate_exact(&mut self, order: u8) -> Result<CursorMut<BlockAdapter>, BlockAllocateError> {
        #[cfg(feature="flame_profile")]
        flame::note("allocate exact", None);

        if order > MAX_ORDER {
            return Err(BlockAllocateError::OrderTooLarge(order));
        }

        #[cfg(feature="flame_profile")]
        flame::note("allocate begin", None);

        let block = BuddyAllocator::find_or_split(&mut self.free, &mut self.tree, order)?;

        // Safe because we have exclusive access to `block`.
        unsafe {
            block.get().unwrap().set_used(true);
        }

        let ptr = block.get().unwrap() as *const _ ;
        self.free[order as usize].remove(ptr);

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
    let allocator = BuddyAllocator::<Vec<*const Block>>::new();
    demo(allocator, print_addresses, blocks, block_size)
}

pub fn demo_linked_lists(print_addresses: bool, blocks: u32, block_size: u8) {
    let allocator = BuddyAllocator::<SinglyLinkedList<BlockPtrAdapter>>::new();
    demo(allocator, print_addresses, blocks, block_size)
}


fn demo<L: FreeList>(
    mut allocator: BuddyAllocator<L>,
    print_addresses: bool,
    blocks: u32,
    block_size: u8,
) {
    let top_level_blocks = top_level_blocks(blocks, block_size);

    for block_number in 0..top_level_blocks {
        allocator.create_top_level(
            2usize.pow(u32::from(MAX_ORDER + MIN_ORDER)) * block_number as usize,
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
        let mut allocator = BuddyAllocator::<Vec<*const Block>>::new();
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
        let mut allocator = BuddyAllocator::<Vec<*const Block>>::new();
        let mut block = allocator.create_top_level(0);
        BuddyAllocator::<Vec<*const Block>>::split(&mut block).unwrap();

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
        let mut allocator = BuddyAllocator::<Vec<*const Block>>::new();
        allocator.create_top_level(0);
        let cursor = allocator.allocate_exact(MAX_ORDER).unwrap();
        let expected_block = Block::new(0, MAX_ORDER, true);
        assert_eq!(*cursor.get().unwrap(), expected_block);
    }

    #[test]
    fn test_allocate_exact_no_free() {
        let mut allocator = BuddyAllocator::<Vec<*const Block>>::new();
        allocator.create_top_level(0);
        let cursor = allocator.allocate_exact(MAX_ORDER - 2).unwrap();
        let expected_block = Block::new(0, MAX_ORDER - 2, true);

        assert_eq!(*cursor.get().unwrap(), expected_block);
    }

    #[test]
    fn test_linked_list_remove() {
        let mut list = SinglyLinkedList::<BlockPtrAdapter>::new(BlockPtrAdapter::new());
        list.push_front(Box::new(BlockPtr::new(1 as *const _)));
        list.push_front(Box::new(BlockPtr::new(2 as *const _)));
        list.push_front(Box::new(BlockPtr::new(3 as *const _)));
        list.push_front(Box::new(BlockPtr::new(4 as *const _)));
        list.push_front(Box::new(BlockPtr::new(5 as *const _)));
        list.remove(2 as *const _).unwrap();

        assert_eq!(
            list.iter().map(|i| i.ptr).collect::<Vec<*const Block>>(),
            vec![5 as *const _, 4 as *const _, 3 as *const _, 1 as *const _]);
    }

    #[test]
    fn test_unique_addresses_vecs() {
        let mut allocator = BuddyAllocator::<Vec<*const Block>>::new();

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
        let mut allocator = BuddyAllocator::<SinglyLinkedList<BlockPtrAdapter>>::new();

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