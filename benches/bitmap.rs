#[macro_use]
extern crate criterion;
extern crate buddy_allocator_workshop;

use criterion::Criterion;

fn bitmap(c: &mut Criterion) {
    use buddy_allocator_workshop::buddy_allocator_bitmap::*;
    use buddy_allocator_workshop::{MAX_ORDER, MIN_ORDER};

    c.bench_function("bitmap allocate_exact", |b| {
        let mut trees = vec![Tree::new(), Tree::new(), Tree::new()];
        let mut current_tree = 0;

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
