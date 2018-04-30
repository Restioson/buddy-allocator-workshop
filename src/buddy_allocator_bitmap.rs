use super::{MAX_ORDER, MIN_ORDER, ORDERS};
#[cfg(feature = "flame_profile")]
use flame;
use flat_tree;
use std::cmp;
///! A modified buddy bitmap allocator
use std::mem;

/// A block in the bitmap
struct Block {
    /// The order of the biggest block under this block + 1
    order_free: u8,
}

// TODO move lock to tree itself
impl Block {
    pub fn new_free(order: u8) -> Self {
        Block {
            order_free: order + 1,
        }
    }

    /// Gets the order of free blocks beneath and including this block if the block is free, else
    /// `None`
    #[inline]
    pub fn order_free(&self) -> Option<u8> {
        if self.order_free == 0 {
            None
        } else {
            Some(self.order_free - 1)
        }
    }

    #[inline]
    fn set(&mut self, new: u8) {
        self.order_free = new;
    }

    /// Set the order of the largest free block under this block
    #[inline]
    pub fn set_free(&mut self, free_order: u8) {
        self.set(free_order + 1);
    }

    #[inline]
    pub fn set_used(&mut self) {
        self.set(0);
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

        for i in 0..BLOCKS_IN_TREE {
            let order = flat_tree::depth(i); // TODO
            flat_blocks[i] = Block::new_free(order as u8);
        }

        Tree { flat_blocks }
    }

    #[cfg_attr(feature = "flame_profile", flame)]
    pub fn alloc_exact(&mut self, desired_order: u8) -> Option<*const u8> {
        let top = flat_tree::index(MAX_ORDER as usize, 0);
        let mut index = top;

        let root = &mut self.flat_blocks[index];

        match root.order_free() {
            Some(o) if o < desired_order => return None,
            None => return None,
            _ => (),
        };

        let mut addr = 0;

        for level in 0..(MAX_ORDER - desired_order) {
            #[cfg(feature = "flame_profile")]
            let _g = flame::start_guard("tree_traverse_loop");

            let left_child =
                &mut self.flat_blocks[flat_tree::left_child(index)
                                          .expect(&format!("{} does not have left child!", index))];

            index = match left_child.order_free() {
                Some(o) if o > desired_order => flat_tree::left_child(index).unwrap(),
                _ => {
                    addr += 1 << ((MAX_ORDER + MIN_ORDER - level - 1) as u32);
                    flat_tree::right_child(index).unwrap()
                }
            };
        }

        let block = &mut self.flat_blocks[index];
        block.set_used();

        // Iterate upwards and set parents accordingly
        let mut index = flat_tree::parent(index);

        for _ in 1..(MAX_ORDER) {
            #[cfg(feature = "flame_profile")]
            let _g = flame::start_guard("traverse_up_loop");
            let (left, right) = (
                &mut self.flat_blocks[flat_tree::left_child(index).unwrap()].order_free(),
                &mut self.flat_blocks[flat_tree::right_child(index).unwrap()].order_free(),
            );

            if let Some(order) = cmp::max(left, right) {
                self.flat_blocks[index].set_free(*order);
            } else {
                self.flat_blocks[index].set_used();
            }

            index = flat_tree::parent(index);
        }

        Some(addr as *const u8)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_blocks_in_tree() {
        assert_eq!(Tree::blocks_in_tree(3), 1 + 2 + 4);
        assert_eq!(Tree::blocks_in_tree(1), 1);
    }

    #[test]
    fn test_alloc_exact() {
        let mut tree = Tree::new();
        tree.alloc_exact(0).unwrap();
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

pub fn demo(print_addresses: bool, blocks: u32, block_size: u8) {
    let mut allocator = Tree::new();

    for _ in 0..blocks {
        let addr = allocator.alloc_exact(block_size).unwrap();

        if print_addresses {
            println!("Address: {:#x}", addr as usize);
        }
    }
}
