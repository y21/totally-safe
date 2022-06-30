# `totally_safe`

Working with raw pointers and other unsafe std APIs in 100% safe Rust code¹. This crate contains 0 lines of unsafe and has `#![forbid(unsafe_code)]` at the top.

It does all of this by manipulating the process memory through the std filesystem APIs, using the magic `/proc/self/mem` file and the seek API to read/write bytes at arbitrary positions. Inspired by [this "unsoundness" issue](https://github.com/rust-lang/rust/issues/32670), which will never be fixed because it is considered [outside of the scope of Rust's memory safety guarantees](https://doc.rust-lang.org/nightly/std/os/unix/io/index.html#procselfmem-and-similar-os-features).

If a function does not exist, remember that you can transmute an already existing `unsafe fn(...P) -> R` to a `fn(...P) -> R`. There is even a "typesafe" function in this crate that does this cast.
```rs
// Turn a highly unsafe function into a safe function
let danger = totally_safe::unsafe_fn_to_safe_fn(std::hint::unreachable_unchecked);
danger();
```
This crate implements `Box<T>` -> `Box<U>` (as well as for Rc and Arc, and potentially more types soon) transmutes using `Box::from_raw(Box::into_raw(ptr).cast())` instead of plain transmutes because `Box<T>` memory layout is unspecified and could change at any point.
```rs
let b1: Box<u32> = Box::new(123);
let b2: Box<i32> = totally_safe::container_transmute(b1);
```
> `container_transmute` is generic over its container and can be applied to many other types. For example, you can also do `Rc<u32>` -> `Rc<i32>` transmutes with `container_transmute`.

----
¹ This crate is a joke and is not meant to be ever used in a real project. 
For this reason I won't publish it on `crates.io`, and it (intentionally) won't even compile in release mode.
It is also (of course) completely unsound, all of the functions can easily cause UB in safe code if misused.

This was more of a fun project/challenge to see how much of the unsafe part of the standard library could be implemented in safe Rust using this (in a stable way, i.e. not relying on std impl details). It ended up being a huge mess but it was fun regardless :)
