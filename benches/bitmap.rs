#[macro_use]
extern crate criterion;
extern crate array_init;
extern crate buddy_allocator_workshop;

use criterion::Criterion;

fn bitmap(c: &mut Criterion) {
    use buddy_allocator_workshop::buddy_allocator_bitmap::*;
    use buddy_allocator_workshop::{MAX_ORDER, MIN_ORDER};

    let mut trees: [Tree; 64] = array_init::array_init(|_| Tree::new());
    let mut current_tree = 0;

    c.bench_function("bitmap allocate_exact", move |b| {
        b.iter(|| {
            match trees[current_tree].alloc_exact(0) {
                Some(_) => (),
                None => {
                    current_tree += 1;
                    trees[current_tree].alloc_exact(0);
                },
            };
        });
    });
}

criterion_group!(benches, bitmap);
criterion_main!(benches);
