///! A modified buddy bitmap allocator

use std::mem;
use std::cmp;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature="flame_profile")]
use flame;
use flat_tree;
use super::{MIN_ORDER, MAX_ORDER, ORDERS, top_level_blocks};

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

    fn set(&mut self, new: u8) {
        self.order_free = new;
    }

    /// Set the order of the largest free block under this block
    pub fn set_free(&mut self, free_order: u8) {
        self.set(free_order + 1);
    }

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
        let mut flat_blocks: Box<[Block; BLOCKS_IN_TREE]> = Box::new(unsafe { mem::uninitialized() });

        for i in 0..BLOCKS_IN_TREE {
            let order = flat_tree::depth(i); // TODO
            flat_blocks[i] = Block::new_free(order as u8);
        }

        Tree {
            flat_blocks,
        }
    }

    pub fn alloc_exact<'a: 'b, 'b>(&'a mut self, desired_order: u8) -> Option<*const u8> {
        let mut index = flat_tree::index(MAX_ORDER as usize, 0);

        let block = &mut self.flat_blocks[index];

        let direction = match block.order_free() {
            Some(o) if o < desired_order => return None,
            None => return None,
            _ => (),
        };

        let mut addr = 0;

        for level in 0..=(MAX_ORDER - desired_order) {
            let block = &mut self.flat_blocks[index];

            match block.order_free() {
                Some(o) if o == desired_order && level == (MAX_ORDER - desired_order) => {
                    block.set_used();

                    // Iterate upwards and set parents accordingly
                    let mut index = flat_tree::parent(index);

                    loop {
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

                        if index == flat_tree::index(MAX_ORDER as usize, 0) {
                            break;
                        }
                    }

                    return Some(addr as *const u8);
                }
                _ => (),
            }

            let left_child = &mut self.flat_blocks[flat_tree::left_child(index)
                .expect(&format!("{} does not have left child!", index))];

            let direction = match left_child.order_free() {
                Some(o) if o > desired_order => BinaryDirection::Left,
                _ => BinaryDirection::Right,
            };

            index = match direction {
                BinaryDirection::Left => flat_tree::left_child(index).unwrap(),
                BinaryDirection::Right => flat_tree::right_child(index).unwrap(),
            };

            if direction == BinaryDirection::Right {
                addr += 2usize.pow((MAX_ORDER + MIN_ORDER - level - 1) as u32)
            }
        }

        // TODO
        panic!("uhh...");
    }
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
enum BinaryDirection {
    Left,
    Right,
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
            println!("addr: {:?}", addr);

            if seen.contains(&addr) {
                panic!("Allocator must return addresses never been allocated before!");
            } else {
                seen.push(addr);
            }
        }
    }
}