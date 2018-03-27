use core::collections::LinkedList;
use super::*;

// number of orders
const ORDERS: u8 = 19;
const MIN_ORDER: u8 = 12;

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

pub struct BuddyAllocator {
    lists: [LinkedList<Block>; ORDERS as usize],
}

/// A very temporary block index. Is not to be trusted to remain pointing to the same block. Use at
/// own risk!
#[derive(Debug, Copy, Clone)]
struct BlockIndex {
    order: u8,
    index: usize,
}

impl BuddyAllocator {
    pub fn new() -> Self {
        BuddyAllocator { lists: array_init::array_init(|_| LinkedList::new()) }
    }

    /// Get the index of a block
    fn index_of(&self, block: &Block) -> Option<BlockIndex> {
        Some(BlockIndex {
            order: block.order,
            index: self.lists[block.order as usize].iter().position(
                |i| i == block,
            )?,
        })
    }

    /// Get a block by its index.
    ///
    /// # Panicking
    ///
    /// Panics if the order is larger than maximum. This indicates a programming error.
    fn get(&self, block: &BlockIndex) -> Option<&Block> {
        let len = self.lists[block.order as usize].len();
        if len == 0 {
            return None;
        }

        if block.index < self.lists[block.order as usize].len() / 2 {
            self.lists[block.order as usize].iter().nth(block.index)
        } else {
            self.lists[block.order as usize].iter().rev().nth(
                len - 1 -
                    block.index,
            )
        }
    }

    /// Get a block by its index mutably.
    ///
    /// # Panicking
    ///
    /// Panics if the order is larger than maximum. This indicates a programming error.
    fn get_mut(&mut self, block: &BlockIndex) -> Option<&mut Block> {
        let len = self.lists[block.order as usize].len();
        if len == 0 {
            return None;
        }

        if block.index < self.lists[block.order as usize].len() / 2 {
            self.lists[block.order as usize].iter_mut().nth(block.index)
        } else {
            self.lists[block.order as usize].iter_mut().rev().nth(
                len - 1 - block.index,
            )
        }
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
        self.lists[ORDERS as usize - 1].push_back(Block {
            begin_address,
            order: ORDERS - 1,
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

        assert_eq!(
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
            let block = Block {
                begin_address: if n == 0 {
                    block.begin_address
                } else {
                    block.begin_address + 2usize.pow((order + MIN_ORDER) as u32)
                },
                order,
                state: BlockState::Free,
            };

            block
        });

        // Remove original block
        let mut second_part = self.lists[original_order as usize].split_off(index.index);
        second_part.pop_front();
        self.lists[original_order as usize].append(&mut second_part);

        let [first, second] = buddies;
        self.lists[order as usize].push_back(first);
        self.lists[order as usize].push_back(second);

        Ok(BlockIndex {
            order,
            index: self.lists[order as usize].len() - 2,
        })
    }

    fn allocate_exact(&mut self, order: u8) -> Result<BlockIndex, BlockAllocateError> {
        let mut index = self.find_or_split(order)?;
        self.modify(&mut index, BlockState::Used);
        Ok(index)
    }

    /// Find a frame of a given order or splits other frames recursively until one is made. Does not
    /// set state to used.
    fn find_or_split(&mut self, order: u8) -> Result<BlockIndex, BlockAllocateError> {
        if order >= ORDERS {
            return Err(BlockAllocateError::OrderTooLarge {
                max: ORDERS - 1,
                received: order,
            });
        }

        let opt: Option<Result<BlockIndex, BlockAllocateError>> = self.lists[order as usize]
            .iter()
            .position(|block| block.state == BlockState::Free)
            .map(|index| BlockIndex { order, index })
            .map(Ok);

        let block = match opt {
            Some(thing) => thing,
            None => {
                if order >= ORDERS - 1 {
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

#[derive(Debug)]
pub enum BlockSplitError {
    BlockSmallestPossible,
}

#[derive(Debug)]
pub enum BlockAllocateError {
    OrderTooLarge { max: u8, received: u8 },
    NoBlocksAvailable,
}

impl PhysicalAllocator for BuddyAllocator {
    fn alloc(&mut self, size: PageSize) -> *const u8 {
        let index = self.allocate_exact(size.get_power_of_two() - MIN_ORDER)
            .unwrap();
        let block = self.get(&index).unwrap();
        block.begin_address as *const u8
    }

    fn dealloc(&mut self, _frame: *const u8) {
        unimplemented!()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_create_top_level() {
        let mut allocator = BuddyAllocator::new();
        allocator.create_top_level(0);
        allocator.create_top_level(2usize.pow((MIN_ORDER + ORDERS - 1) as u32));

        let mut expected = LinkedList::new();
        expected.push_back(Block {
            begin_address: 0,
            order: ORDERS - 1,
            state: BlockState::Free,
        });
        expected.push_back(Block {
            begin_address: 2usize.pow((MIN_ORDER + ORDERS - 1) as u32),
            order: ORDERS - 1,
            state: BlockState::Free,
        });

        assert_eq!(allocator.lists[ORDERS as usize - 1].len(), 2);
        assert_eq!(allocator.lists[ORDERS as usize - 1], expected);
    }

    #[test]
    fn test_split() {
        let mut allocator = BuddyAllocator::new();
        allocator.create_top_level(0);
        allocator
            .split(BlockIndex {
                index: 0,
                order: ORDERS - 1,
            })
            .unwrap();

        let expected_blocks = [
            Block {
                begin_address: 0,
                order: ORDERS - 2,
                state: BlockState::Free,
            },
            Block {
                begin_address: 2usize.pow((MIN_ORDER + ORDERS) as u32 - 2),
                order: ORDERS - 2,
                state: BlockState::Free,
            },
        ];

        assert_eq!(allocator.lists[ORDERS as usize - 1].len(), 0);
        assert_eq!(allocator.lists[ORDERS as usize - 2].len(), 2);

        allocator.lists[ORDERS as usize - 2]
            .iter()
            .zip(expected_blocks.iter())
            .for_each(|(block, expected)| assert_eq!(block, expected));
    }

    #[test]
    fn test_get() {
        let mut allocator = BuddyAllocator::new();
        allocator.create_top_level(0);
        allocator.create_top_level(1024 * 1024 * 1024);

        let mut indices: [BlockIndex; 2] = array_init::array_init(|_| {
            allocator
                .split(BlockIndex {
                    index: 0,
                    order: ORDERS - 1,
                })
                .unwrap()
        });

        indices[1].index += 1; // Make sure we iterate from back too

        let expected_blocks = [
            Block {
                begin_address: 0,
                order: ORDERS - 2,
                state: BlockState::Free,
            },
            Block {
                begin_address: 2usize.pow((MIN_ORDER + ORDERS) as u32 - 2) * indices[1].index,
                order: ORDERS - 2,
                state: BlockState::Free,
            },
        ];

        for (index, expected) in indices.iter().zip(expected_blocks.iter()) {
            let block = allocator.get(index).unwrap();
            assert_eq!(block, expected)
        }
    }

    #[test]
    fn test_get_mut() {
        let mut allocator = BuddyAllocator::new();
        allocator.create_top_level(0);
        allocator.create_top_level(1024 * 1024 * 1024);

        let mut indices: [BlockIndex; 2] = array_init::array_init(|_| {
            allocator
                .split(BlockIndex {
                    index: 0,
                    order: ORDERS - 1,
                })
                .unwrap()
        });

        indices[1].index += 1; // Make sure we iterate from back too

        let expected_blocks = [
            Block {
                begin_address: 0,
                order: ORDERS - 2,
                state: BlockState::Free,
            },
            Block {
                begin_address: 2usize.pow((MIN_ORDER + ORDERS) as u32 - 2) * indices[1].index,
                order: ORDERS - 2,
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
        let mut allocator = BuddyAllocator::new();
        allocator.create_top_level(0);
        let index = allocator.allocate_exact(ORDERS - 1).unwrap();
        let expected_block = Block {
            begin_address: 0,
            order: ORDERS - 1,
            state: BlockState::Used,
        };
        assert_eq!(*allocator.get(&index).unwrap(), expected_block);
    }

    #[test]
    fn test_allocate_exact_no_free() {
        let mut allocator = BuddyAllocator::new();
        allocator.create_top_level(0);
        let index = allocator.allocate_exact(ORDERS - 3).unwrap();
        let expected_block = Block {
            begin_address: 0,
            order: ORDERS - 3,
            state: BlockState::Used,
        };

        assert_eq!(*allocator.get(&index).unwrap(), expected_block);
    }

    // TODO test allocate_exact failing case propagates error right
}
