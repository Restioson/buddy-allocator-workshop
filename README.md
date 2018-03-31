# Buddy Allocator Workshop

This repository contains some small example implementations of
[buddy allocators][buddy memory allocation]. They are designed to
allocate physical memory, although they could be used for other types of
allocation, such as the heap. Eventually, the best performing one will
be merged into [flower][flower].

# Getting Started

First, clone the repo. Then, `cd` into it and do `cargo +nightly run` to
run all the demo allocators. By default, the block size is 4kib and the
amount of blocks is 100 000, so this may take a while for the linked
lists example. Don't worry, it won't actually allocate anything -- only
mock memory blocks. Pass `-h` or `--help` to get help and view the
usage. You can edit the source code to change min/max block sizes, etc.
To run the unit tests, run `cargo test`. Unfortunately there are no
cargo benchmarks yet, but I have benchmarked it rather unscientifically
on my Windows machine.

# Implementations

## Benchmark

I did an unscientific benchmark done on my Windows machine by timing
various implementations using the builtin reporting, allocating a
gibibyte in 4kib blocks (with printing off). If you have any other
benchmarks to add (possibly more scientific ones), please see
[Contributing][contributing section]

### Specifications

![Computer Specifications][specs]

(MSI CX61-2QF)

### Table

| Implementation                | Time  | Throughput      |
|-------------------------------|-------|-----------------|
| Lists - Vectors               | 2 min  | ~8.33e-3 GiB/s |
| Lists - Doubly Linked Lists   | 25min | ~6.66e-4 GiB/s  |
| RB Trees - Vectors            | ~0.3s | ~3.33 GiB/s     |
| RB Trees - Singly Linked Lists| ~0.5s | ~2 GiB/s        |

## Naive List Based Implementation

This implementation keeps a list per order of block. It is generic over
the typeof lists used. I decided to use two kinds of lists: vectors
(`Vec` from `std`), and doubly linked lists (`LinkedList`, also from
`std`). Linked lists are often prized for their predictable push time
(no reallocation necessary for pushing), while vectors have better cache
locality as the elements are allocated in a contiguous memory block. I
used doubly linked lists because they are faster for indexing than
singly linked lists, as they can iterate from the back or front
depending on whether the index is closer to the beginning or end of the
list. I decided to test both to see which would perform better overall.

The implementation is recursive. To allocate a free block of order *k*,
it first searches for any free blocks in the list of order *k* blocks.
It does not keep a free list. If none are found, it recurses by trying
to allocating a block of order *k* + 1. Finally, if at no point were any
free blocks found it gives up and panics. As soon as one is it splits it
in half, removing the original block from it's order list and pushing
the halves to the order list immediately lower. It then returns the
order and index of the first block in its order list. You can find this
algorithm in [`find_or_split`][find_or_split lists].


### Vectors
A quick, un-scientific benchmark on my Windows machine says that it took
around two minutes to allocate a full gibibyte (1024^3 bytes). I did
notice split second pauses every now and again when it had to reallocate
the entire vector to push an element.

### `std`'s Doubly Linked Lists

A similar benchmark says that it took **twenty-five** minutes to
allocate a full gibibyte. This is **over twelve times slower** than
the same implementation with vectors. However, this implementation
wasn't optimised for linked lists, so it is slightly unfair. Unlike the
implementation with vectors, I did not notice any pauses, but allocation
gradually got slower and slower.

----

We can conclude that although doubly linked lists *in theory* are faster
at pushing than vectors are, they were still 12 times slower than
vectors. This could be because the implementation was slightly in favour
of vectors (lots of indexing), or because the vectors had a higher cache
locality and therefore experienced less cache misses, while linked lists
experience high cache misses as they have individually heap-allocated
elements.

## Red-Black Tree

This implementation keeps one red-black tree (from
`intrusive_collections`) for all blocks and a free list for each order.
The free lists were implemented for std's `Vec` and
`intrusive_collections`'s `SinglyLinkedList`. I chose a singly linked
list as there would have been no real benefit to double linking -- the
only method that would have benefited (negligibly so) is
`FreeList::remove`, but this is always called at most on the second
element in this free list, so there is no real point in optimizing this.
The red-black tree individually heap allocates each node, which makes
the cache efficiency worse, but unlike `std`'s `BTreeSet`/`BTreeMap` its
search is `O(log n)`, while `std`'s uses a linear search, which is not
`O(log n)` (you can read about this [here][btreemap]). However, `std`'s
trees do not individually heap allocate nodes, so cache locality is
better. I decided that although this was true, since a buddy allocator
must deal with incredibly large numbers of blocks, it was more important
to have a more efficient search algorithm.

The implementation is recursive. To allocate a free block of order *k*,
it first searches for any free blocks in the free list list of order *k*
blocks. If none are found, it recurses by trying to allocating a block
of order *k* + 1. Finally, if at no point were any free blocks found it
gives up and panics. As soon as one is it splits it in half, removing
the original block from the tree and inserting the halves, pushing their
addresses to the relevant free list. It then returns a cursor pointing
to the first block. You can find this algorithm in
[`find_or_split`][find_or_split trees]. At the outermost layer of
recursion (the function that actually calls the recursive
`find_or_split` function), the returned block is marked as used and
removed from the free list.

### Vectors as Free Lists

Using vectors as free lists took ~0.3s to allocate a full GiB. This is
~0.2s faster than the linked lists as free lists version. This is
probably due to vectors having better cache locality.

### Linked Lists as Free Lists

Using linked lists as free lists took ~0.5s to allocate a full GiB. See
the [Vectors as Free Lists][vectors as free lists] section above.

---

This implementation was *400x faster* than the naive list based
implementation (at best, using vectors as free lists). This is probably
due to red-black trees having `O(log n)` operations across the board,
faster than the searches, inserts, and removes of vectors or linked
lists.

# Contributing

If you have any thing to add (such as an edit to the readme or another
implementation or benchmark) feel free to
[submit a pull request][submit a pr]! You can also
[create an issue][create an issue]. If you just want to chat, feel free
to ping me on the [Rust Discord][rust discord] (Restioson#8323).

[flower]: https://github.com/Restioson/flower
[specs]: https://i.imgur.com/DLLVS55.png
[find_or_split lists]: https://github.com/Restioson/buddy-allocator-workshop/blob/master/src/buddy_allocator_lists.rs#L256
[buddy memory allocation]: https://en.wikipedia.org/wiki/Buddy_memory_allocation
[rust discord]: https://discord.me/rust-lang
[create an issue]: https://github.com/Restioson/buddy-allocator-workshop/issues/new
[submit a pr]: https://github.com/Restioson/buddy-allocator-workshop/compare
[contributing section]: https://github.com/Restioson/buddy-allocator-workshop#contributing
[btreemap]: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
[find_or_split trees]: https://github.com/Restioson/buddy-allocator-workshop/blob/master/src/buddy_allocator_tree.rs#L225
[vectors as free lists]:https://github.com/Restioson/buddy-allocator-workshop#vectors-as-free-lists
