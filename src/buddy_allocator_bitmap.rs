///! A modified buddy bitmap allocator
#[cfg(feature = "flame_profile")]
use flame;
use std::cmp;
use std::mem;
use super::{MAX_ORDER, MIN_ORDER, ORDERS};

/// A block in the bitmap
struct Block {
    /// The order of the biggest block under this block + 1. 0 denotes used
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
    flat_blocks: Box<[Block; Tree::blocks_in_tree(ORDERS)]>,
}

impl Tree {
    const fn blocks_in_tree(levels: u8) -> usize {
        ((1 << levels) - 1) as usize
    }

    pub fn new() -> Tree {
        const BLOCKS_IN_TREE: usize = Tree::blocks_in_tree(ORDERS);
        let mut flat_blocks: Box<[Block; BLOCKS_IN_TREE]> = box unsafe { mem::uninitialized() };

        let mut start: usize = 0;
        for level in 0..ORDERS {
            let order = MAX_ORDER - level;
            let size = 1 << (level as usize);
            for block in start..(start + size) {
                flat_blocks[block] = Block::new_free(order);
            }
            start += size;
        }

        Tree { flat_blocks }
    }

    #[inline]
    fn block_mut(&mut self, index: usize) -> &mut Block {
        unsafe { self.flat_blocks.get_unchecked_mut(index) }
    }

    #[inline]
    fn block(&self, index: usize) -> &Block {
        unsafe { self.flat_blocks.get_unchecked(index) }
    }

    #[cfg_attr(feature = "flame_profile", flame)]
    pub fn alloc_exact(&mut self, desired_order: u8) -> Option<*const u8> {
        let root = &mut self.block_mut(0);

        // If the root node has no orders free, or if it does not have the desired order free
        if root.order_free == 0 || (root.order_free + 1) < desired_order {
            return None;
        }

        let mut addr = 0;
        let mut index = 1;

        for level in 0..(MAX_ORDER - desired_order) {
            #[cfg(feature = "flame_profile")]
            let _loop_guard = flame::start_guard("tree_traverse_loop");

            let left_child_index = flat_tree::left_child(index);
            let left_child = self.block(left_child_index - 1);

            #[cfg(feature = "flame_profile")]
            let _update_guard = flame::start_guard("tree_traverse_update");
            index = match left_child.order_free {
                o if o != 0 && o >= desired_order => left_child_index,
                _ => {
                    addr |= 1 << ((MAX_ORDER + MIN_ORDER - level - 1) as u32);
                    left_child_index + 1
                }
            };
        }

        let block = self.block_mut(index - 1);
        block.order_free = 0;

        // Iterate upwards and set parents accordingly
        for _ in 0..(MAX_ORDER - desired_order) {
            #[cfg(feature = "flame_profile")]
            let traverse_guard = flame::start_guard("traverse_up");

            index = flat_tree::parent(index);

            #[cfg(feature = "flame_profile")]
            traverse_guard.end();

            #[cfg(feature = "flame_profile")]
            let neighbour_guard = flame::start_guard("get_neighbours");

            #[cfg(feature = "flame_profile")]
            let compute_left_index_guard = flame::start_guard("compute_left_index");

            // Treat as right index because we need to be 0 indexed here!
            let right_index = flat_tree::left_child(index);

            #[cfg(feature = "flame_profile")]
            compute_left_index_guard.end();

            let left = self.block(right_index - 1).order_free;
            let right = self.block(right_index).order_free;

            #[cfg(feature = "flame_profile")]
            neighbour_guard.end();

            #[cfg(feature = "flame_profile")]
            let parents_guard = flame::start_guard("update_parents");
            self.block_mut(index - 1).order_free = cmp::max(left, right);

            #[cfg(feature = "flame_profile")]
            parents_guard.end();
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

pub fn demo(print_addresses: bool, blocks: u32, block_size: u8) {
    let mut allocator = Tree::new();

    for _ in 0..blocks {
        let addr = allocator.alloc_exact(block_size).unwrap();

        if print_addresses {
            println!("Address: {:#x}", addr as usize);
        }
    }
}

#[cfg(test)]
mod test {
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
        assert_eq!(tree.alloc_exact(MAX_ORDER - 1), Some((1024usize.pow(3) / 2) as *const u8));
        assert_eq!(tree.alloc_exact(0), None);
    }

    #[test]
    fn test_alloc_unique_addresses() {
        let mut seen = Vec::with_capacity(1000);
        let mut tree = Tree::new();

        println!();

        for _ in 0..1000 {
            let addr = tree.alloc_exact(0).unwrap();

            if seen.contains(&addr) {
                panic!("Allocator must return addresses never been allocated before!");
            } else {
                seen.push(addr);
            }
        }
    }
}
