///! A modified buddy bitmap allocator
use std::cmp;
use std::mem;
use std::time::{Duration, Instant};
use super::{BASE_ORDER, LEVEL_COUNT, MAX_ORDER, TOP_ORDER};

/// A block in the bitmap
struct Block {
    /// The order of the biggest block under this block - 1. 0 denotes used
    order_free: u8,
}

// TODO move lock to tree itself
impl Block {
    pub fn new_free(order: u8) -> Self {
        Block {
            order_free: order + 1,
        }
    }
}

/// A tree of blocks. Contains the flat representation of the tree as a flat array
// TODO i might have a *few* cache misses here, eh?
pub struct Tree {
    /// Flat array representation of tree. Used with the help of the `flat_tree` crate.
    flat_blocks: Box<[Block; Tree::blocks_in_tree(LEVEL_COUNT)]>,
}

impl Tree {
    const fn blocks_in_tree(levels: u8) -> usize {
        ((1 << levels) - 1) as usize
    }

    pub fn new() -> Tree {
        const BLOCKS_IN_TREE: usize = Tree::blocks_in_tree(LEVEL_COUNT);
        let mut flat_blocks: Box<[Block; BLOCKS_IN_TREE]> = box unsafe { mem::uninitialized() };

        let mut start: usize = 0;
        for level in 0..LEVEL_COUNT {
            let order = MAX_ORDER - level;
            let size = 1 << (level as usize);
            for block in start..(start + size) {
                flat_blocks[block] = Block::new_free(order);
            }
            start += size;
        }

        Tree { flat_blocks }
    }

    pub const fn blocks_in_level(order: u8) -> usize {
        (1 << (BASE_ORDER + order) as usize) / (1 << (BASE_ORDER as usize))
    }

    #[inline]
    unsafe fn block_mut(&mut self, index: usize) -> &mut Block {
        debug_assert!(index < Tree::blocks_in_tree(LEVEL_COUNT));
        self.flat_blocks.get_unchecked_mut(index)
    }

    #[inline]
    unsafe fn block(&self, index: usize) -> &Block {
        debug_assert!(index < Tree::blocks_in_tree(LEVEL_COUNT));
        self.flat_blocks.get_unchecked(index)
    }

    pub fn alloc_exact(&mut self, desired_order: u8) -> Option<*const u8> {
        let root = unsafe { self.block_mut(0) };

        // If the root node has no orders free, or if it does not have the desired order free
        if root.order_free == 0 || (root.order_free - 1) < desired_order {
            return None;
        }

        let mut addr: u32 = 0;
        let mut node_index = 1;

        let max_level = MAX_ORDER - desired_order;

        for level in 0..max_level {
            let left_child_index = flat_tree::left_child(node_index);
            let left_child = unsafe { self.block(left_child_index - 1) };

            let o = left_child.order_free;
            // If the child is not used (o!=0) or (desired_order in o-1)
            // Due to the +1 offset, we need to subtract 1 from 0:
            // However, (o - 1) >= desired_order can be simplified to o > desired_order
            node_index = if o != 0 && o > desired_order {
                left_child_index
            } else {
                // Move over to the right: if the parent had a free order and the left didn't, the right must, or the parent is invalid and does not uphold invariants
                // Since the address is moving from the left hand side, we need to increase it
                // Block size in bytes = 2^(BASE_ORDER + order)
                // We also only want to allocate on the order of the child, hence subtracting 1
                addr += 1 << ((TOP_ORDER - level - 1) as u32);
                left_child_index + 1
            };
        }

        let block = unsafe { self.block_mut(node_index - 1) };
        block.order_free = 0;

        // Iterate upwards and set parents accordingly
        for _ in 0..max_level {
            // Treat as right index because we need to be 0 indexed here!
            // If we exclude the last bit, we'll always get an even number (the left node while 1 indexed)
            let right_index = node_index & !1;
            node_index = flat_tree::parent(node_index);

            let left = unsafe { self.block(right_index - 1) }.order_free;
            let right = unsafe { self.block(right_index) }.order_free;

            unsafe { self.block_mut(node_index - 1) }.order_free = cmp::max(left, right);
        }

        Some(addr as *const u8)
    }
}

/// Flat tree things.
///
/// # Note
/// **1 INDEXED!**
mod flat_tree {
    #[inline]
    pub fn left_child(index: usize) -> usize {
        index << 1
    }

    #[inline]
    pub fn parent(index: usize) -> usize {
        index >> 1
    }
}

pub fn demo(print_addresses: bool, blocks: u32, order: u8) -> Duration {
    let num_trees = ((blocks as f32) / (Tree::blocks_in_level(MAX_ORDER - order) as f32)).ceil() as usize;

    let mut trees = Vec::with_capacity(num_trees);
    for _ in 0..num_trees {
        trees.push(Tree::new());
    }

    let start = Instant::now();
    let mut current_tree = 0;

    for _ in 0..blocks {
        let addr = match trees[current_tree].alloc_exact(order) {
            Some(addr) => addr,
            None => {
                current_tree += 1;
                trees[current_tree].alloc_exact(order).unwrap()
            }
        };

        if print_addresses {
            println!("Address: {:#x}", addr as usize);
        }
    }

    start.elapsed()
}

#[cfg(test)]
mod test {
    use std::collections::BTreeSet;
    use super::*;

    #[test]
    fn test_flat_tree_fns() {
        use super::flat_tree::*;
        //    1
        //  2   3
        // 4 5 6 7
        assert_eq!(left_child(1), 2);
        assert_eq!(parent(2), 1);
    }

    #[test]
    fn test_blocks_in_tree() {
        assert_eq!(Tree::blocks_in_tree(3), 1 + 2 + 4);
        assert_eq!(Tree::blocks_in_tree(1), 1);
    }

    #[test]
    fn test_tree_runs_out_of_blocks() {
        let mut tree = Tree::new();
        let max_blocks = Tree::blocks_in_level(MAX_ORDER);
        for _ in 0..max_blocks {
            assert_ne!(tree.alloc_exact(0), None);
        }

        assert_eq!(tree.alloc_exact(0), None);
    }

    #[test]
    fn test_init_tree() {
        let tree = Tree::new();

        // Highest level has 1 block, next has 2, next 4
        assert_eq!(tree.flat_blocks[0].order_free, 19);

        assert_eq!(tree.flat_blocks[1].order_free, 18);
        assert_eq!(tree.flat_blocks[2].order_free, 18);

        assert_eq!(tree.flat_blocks[3].order_free, 17);
        assert_eq!(tree.flat_blocks[4].order_free, 17);
        assert_eq!(tree.flat_blocks[5].order_free, 17);
        assert_eq!(tree.flat_blocks[6].order_free, 17);
    }

    #[test]
    fn test_alloc_exact() {
        let mut tree = Tree::new();
        tree.alloc_exact(3).unwrap();

        tree = Tree::new();
        assert_eq!(tree.alloc_exact(MAX_ORDER - 1), Some(0x0 as *const u8));
        assert_eq!(
            tree.alloc_exact(MAX_ORDER - 1),
            Some((2usize.pow(TOP_ORDER as u32) / 2) as *const u8)
        );
        assert_eq!(tree.alloc_exact(0), None);
        assert_eq!(tree.alloc_exact(MAX_ORDER - 1), None);

        tree = Tree::new();
        assert_eq!(tree.alloc_exact(MAX_ORDER), Some(0x0 as *const u8));
        assert_eq!(tree.alloc_exact(MAX_ORDER), None);
    }

    #[test]
    fn test_alloc_unique_addresses() {
        let max_blocks = Tree::blocks_in_level(MAX_ORDER);
        let mut seen = BTreeSet::new();
        let mut tree = Tree::new();

        for _ in 0..max_blocks {
            let addr = tree.alloc_exact(0).unwrap();

            if seen.contains(&addr) {
                panic!("Allocator must return addresses never been allocated before!");
            } else {
                seen.insert(addr);
            }
        }
    }
}
