use array_init;
use std::collections::LinkedList;
use std::vec::Vec;
use super::{PhysicalAllocator, PageSize, ORDERS, MAX_ORDER, MIN_ORDER, top_level_blocks};

#[derive(Debug, Eq, PartialEq)]
pub struct Block {
    begin_address: usize,
    order: u8,
    state: BlockState,
}

#[repr(u8)]
#[derive(Debug, Eq, PartialEq)]
pub enum BlockState {
    Used,
    Free,
}

pub trait BlockList {
    fn push(&mut self, item: Block);
    fn position<P: FnMut(&Block) -> bool>(&mut self, pred: P) -> Option<usize>;
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> Option<&Block>;
    fn get_mut(&mut self, index: usize) -> Option<&mut Block>;
    fn remove(&mut self, index: usize);
}

impl BlockList for LinkedList<Block> {
    fn push(&mut self, item: Block) {
        self.push_back(item)
    }

    fn len(&self) -> usize {
        LinkedList::len(self)
    }

    fn position<P: FnMut(&Block) -> bool>(&mut self, pred: P) -> Option<usize> {
        self.iter().position(pred)
    }

    fn get(&self, index: usize) -> Option<&Block> {
        let len = self.len();
        if len == 0 {
            return None;
        }

        if index < len / 2 {
            self.iter().nth(index)
        } else {
            self.iter().rev().nth(len - 1 - index)
        }
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut Block> {
        let len = self.len();
        if len == 0 {
            return None;
        }

        if index < len / 2 {
            self.iter_mut().nth(index)
        } else {
            self.iter_mut().rev().nth(len - 1 - index)
        }
    }
    fn remove(&mut self, index: usize) {
        let mut second_part = self.split_off(index);
        second_part.pop_front();
        self.append(&mut second_part);
    }
}

impl BlockList for Vec<Block> {
    fn push(&mut self, item: Block) {
        Vec::push(self, item);
    }

    fn position<P: FnMut(&Block) -> bool>(&mut self, pred: P) -> Option<usize> {
        self.iter().position(pred)
    }

    fn len(&self) -> usize {
        Vec::len(self)
    }

    fn get(&self, index: usize) -> Option<&Block> {
        if self.len() > index {
            Some(&self[index])
        } else {
            None
        }
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut Block> {
        if self.len() > index {
            Some(&mut self[index])
        } else {
            None
        }
    }
    fn remove(&mut self, index: usize) {
        self.remove(index);
    }
}

pub struct BuddyAllocator<L: BlockList> {
    lists: [L; ORDERS as usize],
}

/// A very temporary block index. Is not to be trusted to remain pointing to the same block. Use at
/// own risk!
#[derive(Debug, Copy, Clone)]
struct BlockIndex {
    order: u8,
    index: usize,
}

impl BuddyAllocator<LinkedList<Block>> {
    pub fn new() -> Self {
        BuddyAllocator { lists: array_init::array_init(|_| LinkedList::new()) }
    }
}

impl BuddyAllocator<Vec<Block>> {
    pub fn new() -> Self {
        BuddyAllocator { lists: array_init::array_init(|_| Vec::new()) }
    }
}

impl<L: BlockList> BuddyAllocator<L> {
    /// Get a block by its index.
    ///
    /// # Panicking
    ///
    /// Panics if the order is larger than maximum. This indicates a programming error.
    fn get(&self, block: &BlockIndex) -> Option<&Block> {
        let list = &self.lists[block.order as usize];
        list.get(block.index)
    }

    /// Get a block by its index mutably.
    ///
    /// # Panicking
    ///
    /// Panics if the order is larger than maximum. This indicates a programming error.
    fn get_mut(&mut self, block: &BlockIndex) -> Option<&mut Block> {
        let list = &mut self.lists[block.order as usize];
        list.get_mut(block.index)
    }


    /// Modify a block by setting its state to a new one. This will not merge blocks if set to free,
    /// it will just mark the block as freed.
    ///
    /// # Panicking
    ///
    /// This function will panic if the index is incorrect
    fn modify(&mut self, index: &mut BlockIndex, new_state: BlockState) {
        let block = self.get_mut(index).unwrap();
        block.state = new_state;
    }

    /// Create a top level block
    pub fn create_top_level(&mut self, begin_address: usize) {
        self.lists[MAX_ORDER as usize].push(Block {
            begin_address,
            order: MAX_ORDER,
            state: BlockState::Free,
        });
    }

    /// Splits a block in place. Index will be invalidated. Returns index of first buddy
    ///
    /// # Panicking
    ///
    /// 1. Index incorrect (doesn't point to block or order > max)
    /// 2. Attempt to split used block
    /// 3. List state bad (order x in list order of y != x)
    fn split(&mut self, index: BlockIndex) -> Result<BlockIndex, BlockSplitError> {
        let block = self.get(&index).unwrap();

        if block.state == BlockState::Used {
            panic!("Attempted to split used block at index {:?}", index);
        }

        debug_assert_eq!(
            block.order,
            index.order,
            "Index should have order equal to block!"
        );


        let original_order = block.order;
        let order = original_order - 1;

        if index.order == 0 {
            return Err(BlockSplitError::BlockSmallestPossible);
        }

        let buddies: [Block; 2] = array_init::array_init(|n| {
            Block {
                begin_address: if n == 0 {
                    block.begin_address
                } else {
                    block.begin_address + 2usize.pow(u32::from(order + MIN_ORDER))
                },
                order,
                state: BlockState::Free,
            }
        });

        self.lists[original_order as usize].remove(index.index);

        let [first, second] = buddies;
        self.lists[order as usize].push(first);
        self.lists[order as usize].push(second);

        Ok(BlockIndex {
            order,
            index: self.lists[order as usize].len() - 2,
        })
    }

    fn allocate_exact(&mut self, order: u8) -> Result<BlockIndex, BlockAllocateError> {
        if order > MAX_ORDER {
            return Err(BlockAllocateError::OrderTooLarge(order));
        }

        let mut index = self.find_or_split(order)?;

        self.modify(&mut index, BlockState::Used);
        Ok(index)
    }

    /// Find a frame of a given order or splits other frames recursively until one is made. Does not
    /// set state to used.
    ///
    /// # Panicking
    ///
    /// Panics if the order is greater than max or if a programming error is encountered such as
    /// attempting to split a block of the smallest possible size.
    fn find_or_split(&mut self, order: u8) -> Result<BlockIndex, BlockAllocateError> {
        if order > MAX_ORDER {
            panic!("Order {} larger than max of {}!", order, MAX_ORDER);
        }

        let opt: Option<BlockIndex> = self.lists[order as usize]
            .position(|block| block.state == BlockState::Free)
            .map(|index| BlockIndex { order, index });

        let block = match opt {
            Some(thing) => Ok(thing),
            None => {
                if order >= MAX_ORDER {
                    Err(BlockAllocateError::NoBlocksAvailable)
                } else {
                    let block_index = self.find_or_split(order + 1)?;
                    let first = self.split(block_index).unwrap();
                    Ok(first)
                }
            }
        }?;

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

impl<L: BlockList> PhysicalAllocator for BuddyAllocator<L> {
    fn alloc(&mut self, size: PageSize) -> *const u8 {
        let index = self.allocate_exact(size.power_of_two() - MIN_ORDER)
            .unwrap();
        let block = self.get(&index).unwrap();
        block.begin_address as *const u8
    }

    fn dealloc(&mut self, _frame: *const u8) {
        unimplemented!()
    }
}

pub fn demo_linked_lists(print_addresses: bool, blocks: u32, block_size: u8) {
    let allocator = BuddyAllocator::<LinkedList<Block>>::new();
    demo(allocator, print_addresses, blocks, block_size)
}

pub fn demo_vecs(print_addresses: bool, blocks: u32, block_size: u8) {
    let allocator = BuddyAllocator::<Vec<Block>>::new();
    demo(allocator, print_addresses, blocks, block_size)
}

fn demo<L: BlockList>(
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
        let index = allocator.allocate_exact(block_size).unwrap();
        let addr = allocator.get(&index).unwrap().begin_address;

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
        let mut allocator = BuddyAllocator::<Vec<Block>>::new();
        allocator.create_top_level(0);
        allocator.create_top_level(2usize.pow((MIN_ORDER + MAX_ORDER) as u32));

        let expected = vec![
            Block {
                begin_address: 0,
                order: MAX_ORDER,
                state: BlockState::Free,
            },
            Block {
                begin_address: 2usize.pow((MIN_ORDER + MAX_ORDER) as u32),
                order: MAX_ORDER,
                state: BlockState::Free,
            },
        ];

        assert_eq!(allocator.lists[MAX_ORDER as usize - 1].len(), 0);
        assert_eq!(allocator.lists[MAX_ORDER as usize], expected);
    }

    #[test]
    fn test_split() {
        let mut allocator = BuddyAllocator::<Vec<Block>>::new();
        allocator.create_top_level(0);
        allocator
            .split(BlockIndex {
                index: 0,
                order: MAX_ORDER,
            })
            .unwrap();

        let expected_blocks = [
            Block {
                begin_address: 0,
                order: MAX_ORDER - 1,
                state: BlockState::Free,
            },
            Block {
                begin_address: 2usize.pow((MIN_ORDER + MAX_ORDER) as u32 - 1),
                order: MAX_ORDER - 1,
                state: BlockState::Free,
            },
        ];

        assert_eq!(allocator.lists[MAX_ORDER as usize - 1].len(), 2);
        assert_eq!(allocator.lists[MAX_ORDER as usize].len(), 0);

        allocator.lists[MAX_ORDER as usize - 1]
            .iter()
            .zip(expected_blocks.iter())
            .for_each(|(block, expected)| assert_eq!(block, expected));
    }

    #[test]
    fn test_get_linked_list() {
        let mut allocator = BuddyAllocator::<LinkedList<Block>>::new();
        allocator.create_top_level(0);
        allocator.create_top_level(2usize.pow((MAX_ORDER + MIN_ORDER) as u32) as usize);

        let mut indices: [BlockIndex; 2] = array_init::array_init(|_| {
            allocator
                .split(BlockIndex {
                    index: 0,
                    order: MAX_ORDER,
                })
                .unwrap()
        });

        indices[1].index += 1; // Make sure we iterate from back too

        let expected_blocks = [
            Block {
                begin_address: 0,
                order: MAX_ORDER - 1,
                state: BlockState::Free,
            },
            Block {
                begin_address: 2usize.pow((MIN_ORDER + MAX_ORDER) as u32 - 1) * indices[1].index,
                order: MAX_ORDER - 1,
                state: BlockState::Free,
            },
        ];

        for (index, expected) in indices.iter().zip(expected_blocks.iter()) {
            let block = allocator.get(index).unwrap();
            assert_eq!(block, expected)
        }
    }

    #[test]
    fn test_get_mut_linked_list() {
        let mut allocator = BuddyAllocator::<LinkedList<Block>>::new();
        allocator.create_top_level(0);
        allocator.create_top_level(1024 * 1024 * 1024);

        let mut indices: [BlockIndex; 2] = array_init::array_init(|_| {
            allocator
                .split(BlockIndex {
                    index: 0,
                    order: MAX_ORDER,
                })
                .unwrap()
        });

        indices[1].index += 1; // Make sure we iterate from back too

        let expected_blocks = [
            Block {
                begin_address: 0,
                order: MAX_ORDER - 1,
                state: BlockState::Free,
            },
            Block {
                begin_address: 2usize.pow((MIN_ORDER + MAX_ORDER - 1) as u32) * indices[1].index,
                order: MAX_ORDER - 1,
                state: BlockState::Free,
            },
        ];

        for (index, expected) in indices.iter().zip(expected_blocks.iter()) {
            let block = allocator.get_mut(index).unwrap();
            assert_eq!(block, expected)
        }
    }

    #[test]
    fn test_allocate_exact_with_free() {
        let mut allocator = BuddyAllocator::<Vec<Block>>::new();
        allocator.create_top_level(0);
        let index = allocator.allocate_exact(MAX_ORDER).unwrap();
        let expected_block = Block {
            begin_address: 0,
            order: MAX_ORDER,
            state: BlockState::Used,
        };
        assert_eq!(*allocator.get(&index).unwrap(), expected_block);
    }

    #[test]
    fn test_allocate_exact_no_free() {
        let mut allocator = BuddyAllocator::<Vec<Block>>::new();
        allocator.create_top_level(0);
        let index = allocator.allocate_exact(MAX_ORDER - 2).unwrap();
        let expected_block = Block {
            begin_address: 0,
            order: MAX_ORDER - 2,
            state: BlockState::Used,
        };

        assert_eq!(*allocator.get(&index).unwrap(), expected_block);
    }

    #[test]
    fn test_unique_addresses_linked_lists() {
        let mut allocator = BuddyAllocator::<LinkedList<Block>>::new();

        for block_number in 0..top_level_blocks(1000, 0) {
            allocator.create_top_level(
                2usize.pow((MAX_ORDER + MIN_ORDER) as u32) * block_number as usize,
            );
        }
        let mut seen = Vec::with_capacity(1000);
        for _ in 0..1000 {
            let index = allocator.allocate_exact(0).unwrap();
            let addr = allocator.get(&index).unwrap().begin_address;

            if seen.contains(&addr) {
                panic!("Allocator must return addresses never been allocated before!");
            } else {
                seen.push(addr);
            }
        }
    }

    #[test]
    fn test_unique_addresses_vecs() {
        let mut allocator = BuddyAllocator::<Vec<Block>>::new();

        for block_number in 0..top_level_blocks(1000, 0) {
            allocator.create_top_level(
                2usize.pow((MAX_ORDER + MIN_ORDER) as u32) * block_number as usize,
            );
        }

        let mut seen = Vec::with_capacity(1000);
        for _ in 0..1000 {
            let index = allocator.allocate_exact(0).unwrap();
            let addr = allocator.get(&index).unwrap().begin_address;

            if seen.contains(&addr) {
                panic!("Allocator must return addresses never been allocated before!");
            } else {
                seen.push(addr);
            }
        }
    }

    // TODO test allocate_exact failing case propagates error right
}
