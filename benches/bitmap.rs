#[macro_use]
extern crate criterion;
extern crate buddy_allocator_workshop;

use criterion::Criterion;

fn bitmap(c: &mut Criterion) {
    use buddy_allocator_workshop::buddy_allocator_bitmap::*;
    use buddy_allocator_workshop::{MAX_ORDER, MIN_ORDER};

    c.bench_function("bitmap allocate_exact", |b| {
        let mut tree = Tree::new();

        b.iter(|| {
            match tree.alloc_exact(0) {
                Some(_) => (),
                None => panic!("RIP"),
            };
        });
    });
}

criterion_group!(benches, bitmap);
criterion_main!(benches);
