#[macro_use]
extern crate criterion;
extern crate buddy_allocator_workshop;

use criterion::Criterion;

fn rb_tree_vecs(c: &mut Criterion) {
    use buddy_allocator_workshop::buddy_allocator_tree::*;
    use buddy_allocator_workshop::{MAX_ORDER, MIN_ORDER};

    c.bench_function("rb_tree_vecs allocate_exact", |b| {
        let allocator = &mut BuddyAllocator::<Vec<*const Block>>::new();
        allocator.create_top_level(0);
        allocator.create_top_level(2usize.pow((MIN_ORDER + MAX_ORDER) as u32));

        let mut blocks_created_top_level = 1;

        b.iter(|| {
            match allocator.allocate_exact(0) {
                Ok(_) => (),
                Err(BlockAllocateError::NoBlocksAvailable) => {
                    let size_of_block = 2usize.pow((MIN_ORDER + MAX_ORDER) as u32);
                    allocator.create_top_level(size_of_block * blocks_created_top_level);
                    blocks_created_top_level += 1;
                }
                Err(e) => panic!("Error: {:?}", e),
            };
        });
    });
}

criterion_group!(benches, rb_tree_vecs);
criterion_main!(benches);
