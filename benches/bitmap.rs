#[macro_use]
extern crate criterion;
extern crate array_init;
extern crate buddy_allocator_workshop;

use criterion::Criterion;

fn bitmap(c: &mut Criterion) {
    use buddy_allocator_workshop::buddy_allocator_bitmap::*;
    use buddy_allocator_workshop::{MAX_ORDER, MIN_ORDER};

    let mut tree = Tree::new();

    c.bench_function("bitmap allocate_exact", move |b| {
        b.iter(|| {
            match tree.alloc_exact(0) {
                Some(_) => (),
                None => {
                    tree = Tree::new();
                    tree.alloc_exact(0);
                },
            };
        });
    });
}

criterion_group!(benches, bitmap);
criterion_main!(benches);
