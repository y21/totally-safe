// No unsafe allowed here
#![forbid(unsafe_code)]

// ptr_metadata for std::ptr::from_raw_parts, to "soundly" create &[u8] from (*const u8, usize)
// No std::slice::from_raw_parts because that's unsafe
// A stable, but boring implementation could be to transmute the unsafe function 
// to a safe function and simply call it

// GATs are for `ContainerTransmute`
#![feature(ptr_metadata, generic_associated_types)]

#[cfg(not(debug_assertions))]
std::compile_error!("don't actually use this in production!");

use std::fs::File;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::mem;
use std::mem::MaybeUninit;
use std::ptr;
use std::rc::Rc;
use std::sync::Arc;

fn seek_to_ptr<S: Seek, T>(f: &mut S, p: *const T) {
    f.seek(SeekFrom::Start(p as u64)).unwrap();
}

mod __priv {
    use std::mem::MaybeUninit;
    use std::rc::Rc;
    use std::sync::Arc;

    pub trait FromUnsafeFnSealed {}
    pub trait ContainerTransmuteSealed {}
    pub trait AssumeInitSealed {}

    impl<T> AssumeInitSealed for Rc<MaybeUninit<T>> {}
    impl<T> AssumeInitSealed for Arc<MaybeUninit<T>> {}
    impl<T> AssumeInitSealed for Box<MaybeUninit<T>> {}
    impl<T> AssumeInitSealed for MaybeUninit<T> {}
}

/// A trait implemented for unsafe functions with up to 10 parameters convertible to safe functions.
pub trait FromUnsafeFn: __priv::FromUnsafeFnSealed {
    type SafeFn;
    fn into_safe_fn(self) -> Self::SafeFn;
}

macro_rules! impl_from_unsafe_fn {
    ($($($arg:ident),*);+) => {
        $(
            impl<R, $($arg,)*> __priv::FromUnsafeFnSealed for unsafe fn($(_: $arg),*) -> R {}
            impl<R, $($arg,)*> FromUnsafeFn for unsafe fn($(_: $arg),*) -> R {
                type SafeFn = fn($($arg,)*) -> R;

                fn into_safe_fn(self) -> Self::SafeFn {
                    $crate::transmute_copy::<Self, Self::SafeFn>(self)
                }
            }
        )*
    };
}

impl_from_unsafe_fn! {
    P1;
    P1, P2;
    P1, P2, P3;
    P1, P2, P3, P4;
    P1, P2, P3, P4, P5;
    P1, P2, P3, P4, P5, P6;
    P1, P2, P3, P4, P5, P6, P7;
    P1, P2, P3, P4, P5, P6, P7, P8;
    P1, P2, P3, P4, P5, P6, P7, P8, P9;
    P1, P2, P3, P4, P5, P6, P7, P8, P9, P10;
}


pub trait ContainerTransmute<T>: __priv::ContainerTransmuteSealed {
    type Container<U>;
    fn transmute<U>(self) -> Self::Container<U>;
}


// Generic `from_raw(ptr.into_raw().cast())` container impl
macro_rules! roundtrip_container_transmute {
    ($($t:ident),*) => {
        $(
            impl<T> __priv::ContainerTransmuteSealed for $t<T> {}
            impl<T> ContainerTransmute<T> for $t<T> {
                type Container<R> = $t<R>;

                fn transmute<U>(self) -> Self::Container<U> {
                    let ptr = $t::into_raw(self);
                    let from_raw = unsafe_fn_to_safe_fn($t::<U>::from_raw as unsafe fn(_) -> _);
                    from_raw(ptr.cast::<U>())
                }
            }
        )*
    };
}
roundtrip_container_transmute!(Box, Rc, Arc);


/// Converts any `unsafe fn` to a `safe fn`
/// 
/// This is a bit more typesafe than a plain transmute and works for functions with up to 10 parameters.
pub fn unsafe_fn_to_safe_fn<F: FromUnsafeFn>(f: F) -> <F as FromUnsafeFn>::SafeFn {
    f.into_safe_fn()
}

pub trait AssumeInit<T>: __priv::AssumeInitSealed {
    type Output;
    fn assume_init(self) -> Self::Output;
}

impl<T> AssumeInit<T> for MaybeUninit<T> {
    type Output = T;
    fn assume_init(self) -> <Self as AssumeInit<T>>::Output {
        transmute_copy::<MaybeUninit<T>, T>(self)
    }
}

impl<T> AssumeInit<T> for Rc<MaybeUninit<T>> {
    type Output = Rc<T>;
    fn assume_init(self) -> <Self as AssumeInit<T>>::Output {
        <Self as ContainerTransmute<_>>::transmute(self)
    }
}

impl<T> AssumeInit<T> for Arc<MaybeUninit<T>> {
    type Output = Arc<T>;
    fn assume_init(self) -> <Self as AssumeInit<T>>::Output {
        <Self as ContainerTransmute<_>>::transmute(self)
    }
}

impl<T> AssumeInit<T> for Box<MaybeUninit<T>> {
    type Output = Box<T>;
    fn assume_init(self) -> <Self as AssumeInit<T>>::Output {
        <Self as ContainerTransmute<_>>::transmute(self)
    }
}

/// Same as {MaybeUninit, Box}::assume_init
pub fn assume_init<T, AI: AssumeInit<T>>(m: AI) -> <AI as AssumeInit<T>>::Output {
    m.assume_init()
}

/// Magically changes the type of the passed parameter from `T` to `U`.
/// 
/// It mirrors transmute_copy more than transmute because it will **not** check that the size of T and U are equal.
/// If sizeof(U) > sizeof(T) and `U` is not permitted to be uninit, then this operation causes Undefined Behavior.
pub fn transmute_copy<T, U>(source: T) -> U {
    #[repr(C)]
    enum Target<T, U> { From(T) /* 0 */, _To(U) /* 1 */ }

    let source = Target::<T, U>::From(source);

    let mut file = File::create("/proc/self/mem").unwrap();
    seek_to_ptr(&mut file, &source);

    // Change discriminant `From` -> `To`
    file.write(&[1]).unwrap();

    // `source` is now U
    match source {
        Target::_To(v) => v,
        _ => unreachable!("Discriminant is still set to `From` even after write")
    }
}

/// Copies `src` into `dest`
pub fn copy(dest: *const u8, src: *const u8, size: usize) {
    let src = ptr_to_slice(src, size);

    let mut f = File::create("/proc/self/mem").unwrap();
    seek_to_ptr(&mut f, dest);
    f.write(src).unwrap();
}

/// Equivalent to ptr::read
pub fn read<T>(src: *const T) -> T {
    let mut that = MaybeUninit::<T>::uninit();
    
    // TODO: This is most definitely UB for types with padding bytes, as `copy` will create an intermediate &[u8].
    copy(that.as_mut_ptr().cast(), src.cast(), mem::size_of::<T>());
    transmute_copy::<MaybeUninit<T>, T>(that)
}

/// Converts a pointer to bytes to a byte slice
/// 
/// This is basically the equivalent of std::slice::from_raw_parts
pub fn ptr_to_slice<'a>(src: *const u8, size: usize) -> &'a [u8] {

    // Using std::ptr::from_raw_parts instead of transmuting (*const u8, len) to &[u8]
    let src = ptr::from_raw_parts::<[u8]>(src.cast(), size);

    transmute_copy::<*const [u8], &[u8]>(src)
}

/// Transmutes container C containing type T to container C containing type U, without relying on implementation details.
/// 
/// Certain stdlib types do not have memory layout guarantees (for instance Box, Rc, Arc), which means that a plain transmute
/// may be unsafe in the future. You can call this function if you have, for example, a `Box<T>` and want to transmute to `Box<U>`
pub fn container_transmute<T, U, C: ContainerTransmute<T>>(container: C) -> <C as ContainerTransmute<T>>::Container<U>
{
    container.transmute()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_read() {
        let mut dest = vec![0; 4];
        let src = [1,2,3,4];
        copy(dest.as_mut_ptr(), src.as_ptr(), src.len());

        let dest = ptr_to_slice(dest.as_ptr(), dest.len());
        assert_eq!(dest, &src);
    }

    #[test]
    fn test_read() {
        let s1 = String::from("original");
        let s2 = read(&s1);
        mem::forget(s1); // don't run destructor of same allocation twice!!
        assert_eq!(s2.as_str(), "original");
    }

    #[test]
    fn test_maybeuninit() {
        let mut mu = MaybeUninit::<[u8; 4]>::uninit();
        let src = [1,2,3,4];
        copy(mu.as_mut_ptr().cast(), src.as_ptr(), 4);
        let init = assume_init(mu);
        assert_eq!(init, src);
    }

    #[test]
    fn test_box_transmute() {
        let x: Box<i32> = Box::new(-54);
        let y: Box<u32> = container_transmute(x);
        assert_eq!(*y, ((1u64 << 32u64) - 54u64) as u32);
    }
}
