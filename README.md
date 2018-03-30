# Buddy Allocator Workshop

This repository contains some small example implementations of [buddy
allocators][buddy memory allocation]. They are designed to allocate
physical memory, although they could be used for other types of
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
various implementations with Powershell's `Measure-Command` allocating
a gibibyte in 4kib blocks. If you have any other benchmarks to add
(possibly more scientific ones), please see
[Contributing][contributing section]

### Specifications

![Computer Specifications][specs]

(MSI CX61-2QF)

### Table

| Implementation              | Time  |
|-----------------------------|-------|
| Lists - Vectors             | 2min  |
| Lists - Doubly Linked Lists | 25min |

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
If none are found, it tries to repeat by allocating a block of order
*k* + 1. Finally, if at no point were any free blocks found it gives up
and panics. As soon as one is it splits it in half, removing the
original block from it's order list and pushing the halves to the order
list immediately lower. It then returns the order and index of the first
block in its order list. You can find this algorithm in
[`find_or_split`][find_or_split lists].


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

# Contributing

If you have any thing to add (such as an edit to the readme or another
implementation or benchmark) feel free to [submit a pull request][submit a
pr]! You can also [create an issue][create an issue]. If you just want
to chat, feel free to ping me on the [Rust Discord][rust discord]
(Restioson#8323).

[flower]: https://github.com/Restioson/flower
[specs]: https://i.imgur.com/DLLVS55.png
[find_or_split lists]: https://github.com/Restioson/buddy-allocator-workshop/blob/master/src/buddy_allocator_lists.rs#L256
[buddy memory allocation]: https://en.wikipedia.org/wiki/Buddy_memory_allocation
[rust discord]: https://discord.me/rust-lang
[create an issue]: https://github.com/Restioson/buddy-allocator-workshop/issues/new
[submit a pr]: https://github.com/Restioson/buddy-allocator-workshop/compare
[contributing section]: https://github.com/Restioson/buddy-allocator-workshop#contributing
