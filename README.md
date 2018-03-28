# Buddy Allocator Workshop

This repository contains some small example implementations of buddy
allocators. They are designed to allocate physical memory. Eventually,
the best performing one will be merged into
[flower](https://github.com/Restioson/flower)

# Implementations

## List Based

A list per order of block.

### Vectors

A quick, un-scientific benchmark on my Windows machine says that it took
around two minutes to allocate a full gibbibyte (1024^3 bytes). I did
notice split second pauses every now and again when it had to reallocate
the entire vector to push an element.

### `std`'s Doubly Linked Lists

A similar benchmark says that it took **twenty-five** minutes to
allocate a full gibbibyte. This is **over twelve times slower** than
the same implementation with vectors. However, this implementation
wasn't optimised for linked lists, so it is slightly unfair. Unlike the
implementation with vectors, I did not notice any pauses, but allocation
gradually got slower and slower.