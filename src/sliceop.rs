use crate::utility::*;
use std::mem::MaybeUninit;

#[inline]
pub(crate) fn sub_slice(lhs: &mut [f32], rhs: &[f32]) {
    lhs.iter_mut().zip(rhs).for_each(|(l, r)| *l -= *r);
}

#[inline]
pub(crate) fn mul_slice(lhs: &mut [f32], rhs: &[f32]) {
    lhs.iter_mut().zip(rhs).for_each(|(l, r)| *l *= *r);
}

#[inline]
pub(crate) fn div_slice(lhs: &mut [f32], rhs: &[f32], default: f32) {
    lhs.iter_mut()
        .zip(rhs)
        .for_each(|(l, r)| *l = if is_zero(*r) { default } else { *l / *r });
}

#[inline]
pub(crate) fn div_slice_uninit(
    dst: &mut [MaybeUninit<f32>],
    lhs: &[f32],
    rhs: &[f32],
    default: f32,
) {
    dst.iter_mut()
        .zip(lhs.iter().zip(rhs))
        .for_each(|(d, (l, r))| {
            d.write(if is_zero(*r) { default } else { *l / *r });
        });
}

/// Multiply a source slice by a scalar and store in a destination slice
#[inline]
pub(crate) fn mul_slice_scalar_uninit(dst: &mut [MaybeUninit<f32>], src: &[f32], scalar: f32) {
    dst.iter_mut().zip(src).for_each(|(d, s)| {
        d.write(*s * scalar);
    });
}

/// Compute a _strided summation_ of `f32` elements in `src`, where the stride
/// length is `dst.len()`.
///
/// In more detail, break source slice `src` into `N` chunks `C0...CN-1`, where
/// `N = dst.len()`, and set the `i`th element of `dst` to be the sum of the
/// `i`th element of each chunk `Ck`:
///
/// - `dst[0] = SUM(k=0..N-1, Ck[0])`
/// - `dst[1] = SUM(k=0..N-1, Ck[1])`
/// - `dst[2] = SUM(k=0..N-1, Ck[2])`
/// - ...
#[inline]
pub(crate) fn sum_slices_uninit<'a>(dst: &'a mut [MaybeUninit<f32>], src: &[f32]) -> &'a mut [f32] {
    let len = dst.len();
    dst.iter_mut().zip(src).for_each(|(d, s)| {
        d.write(*s);
    });
    let dst = unsafe { &mut *(dst as *mut _ as *mut [f32]) };
    src[len..].chunks_exact(len).for_each(|s| {
        dst.iter_mut().zip(s).for_each(|(d, s)| {
            *d += *s;
        });
    });
    dst
}

/// Compute a _strided summation_ of `f32` elements in `src`, where the stride
/// length is `dst.len()`, and store as `f64` in `dst`.
///
/// In more detail, break source slice `src` into `N` chunks `C0...CN-1`, where
/// `N = dst.len()`, and set the `i`th element of `dst` to be the sum of the
/// `i`th element of each chunk `Ck`:
///
/// - `dst[0] = SUM(k=0..N-1, Ck[0])`
/// - `dst[1] = SUM(k=0..N-1, Ck[1])`
/// - `dst[2] = SUM(k=0..N-1, Ck[2])`
/// - ...
#[inline]
pub(crate) fn sum_slices_f64_uninit<'a>(
    dst: &'a mut [MaybeUninit<f64>],
    src: &[f32],
) -> &'a mut [f64] {
    let len = dst.len();
    dst.iter_mut().zip(src).for_each(|(d, s)| {
        d.write(*s as f64);
    });
    let dst = unsafe { &mut *(dst as *mut _ as *mut [f64]) };
    src[len..].chunks_exact(len).for_each(|s| {
        dst.iter_mut().zip(s).for_each(|(d, s)| {
            *d += *s as f64;
        });
    });
    dst
}

#[inline]
pub(crate) fn fma_slices_uninit<'a>(
    dst: &'a mut [MaybeUninit<f32>],
    src1: &[f32],
    src2: &[f32],
) -> &'a mut [f32] {
    let len = dst.len();
    dst.iter_mut()
        .zip(src1.iter().zip(src2))
        .for_each(|(d, (s1, s2))| {
            d.write(*s1 * *s2);
        });
    let dst = unsafe { &mut *(dst as *mut _ as *mut [f32]) };
    src1[len..]
        .chunks_exact(len)
        .zip(src2[len..].chunks_exact(len))
        .for_each(|(s1, s2)| {
            dst.iter_mut()
                .zip(s1.iter().zip(s2))
                .for_each(|(d, (s1, s2))| {
                    *d += *s1 * *s2;
                });
        });
    dst
}

#[inline]
pub(crate) fn max_slices_uninit<'a>(dst: &'a mut [MaybeUninit<f32>], src: &[f32]) -> &'a mut [f32] {
    let len = dst.len();
    dst.iter_mut().zip(src).for_each(|(d, s)| {
        d.write(*s);
    });
    let dst = unsafe { &mut *(dst as *mut _ as *mut [f32]) };
    src[len..].chunks_exact(len).for_each(|s| {
        dst.iter_mut().zip(s).for_each(|(d, s)| {
            *d = max(*d, *s);
        });
    });
    dst
}

#[inline]
pub(crate) fn max_fma_slices_uninit<'a>(
    dst: &'a mut [MaybeUninit<f32>],
    src1: &[f32],
    src2: &[f32],
) -> &'a mut [f32] {
    let len = dst.len();
    dst.iter_mut()
        .zip(src1.iter().zip(src2))
        .for_each(|(d, (s1, s2))| {
            d.write(if s2.is_sign_positive() {
                *s1 * *s2
            } else {
                *s1
            });
        });
    let dst = unsafe { &mut *(dst as *mut _ as *mut [f32]) };
    src1[len..]
        .chunks_exact(len)
        .zip(src2[len..].chunks_exact(len))
        .for_each(|(s1, s2)| {
            dst.iter_mut()
                .zip(s1.iter().zip(s2))
                .for_each(|(d, (s1, s2))| {
                    if s2.is_sign_positive() {
                        *d += *s1 * *s2;
                    } else {
                        *d = max(*d, *s1);
                    }
                });
        });
    dst
}

#[inline]
pub(crate) fn inner_product(src1: &[f32], src2: &[f32]) -> f32 {
    const CHUNK_SIZE: usize = 8;

    let len = src1.len();
    let len_chunk = len / CHUNK_SIZE * CHUNK_SIZE;
    let mut acc = [0.0; CHUNK_SIZE];

    for i in (0..len_chunk).step_by(CHUNK_SIZE) {
        for j in 0..CHUNK_SIZE {
            unsafe {
                let x = *src1.get_unchecked(i + j);
                let y = *src2.get_unchecked(i + j);
                *acc.get_unchecked_mut(j) += (x * y) as f64;
            }
        }
    }

    for i in len_chunk..len {
        unsafe {
            let x = *src1.get_unchecked(i);
            let y = *src2.get_unchecked(i);
            *acc.get_unchecked_mut(0) += (x * y) as f64;
        }
    }

    acc.iter().sum::<f64>() as f32
}

#[inline]
pub(crate) fn inner_product_cond(
    src1: &[f32],
    src2: &[f32],
    cond: &[u16],
    threshold: u16,
    less: f32,
    greater: f32,
    equal: f32,
) -> f32 {
    const CHUNK_SIZE: usize = 8;

    let len = src1.len();
    let len_chunk = len / CHUNK_SIZE * CHUNK_SIZE;
    let mut acc = [0.0; CHUNK_SIZE];

    for i in (0..len_chunk).step_by(CHUNK_SIZE) {
        for j in 0..CHUNK_SIZE {
            unsafe {
                let x = *src1.get_unchecked(i + j);
                let y = *src2.get_unchecked(i + j);
                let c = *cond.get_unchecked(i + j);

                // `match` prevents vectorization
                #[allow(clippy::comparison_chain)]
                let z = if c < threshold {
                    less
                } else if c > threshold {
                    greater
                } else {
                    equal
                };

                *acc.get_unchecked_mut(j) += (x * y * z) as f64;
            }
        }
    }

    for i in len_chunk..len {
        unsafe {
            let x = *src1.get_unchecked(i);
            let y = *src2.get_unchecked(i);
            let c = *cond.get_unchecked(i);

            #[allow(clippy::comparison_chain)]
            let z = if c < threshold {
                less
            } else if c > threshold {
                greater
            } else {
                equal
            };

            *acc.get_unchecked_mut(0) += (x * y * z) as f64;
        }
    }

    acc.iter().sum::<f64>() as f32
}

/// Extract a reference to a specific "row" from a one-dimensional slice, where
/// the data is conceptually arranged as a two-dimensional array.
///
/// # Arguments
///
/// * `slice` - slice to extract a reference from
/// * `index` - the index of the conceptual "row" to reference
/// * `row_size` - the size of the conceptual "row" to reference
#[inline]
pub(crate) fn row<T>(slice: &[T], index: usize, row_size: usize) -> &[T] {
    &slice[index * row_size..(index + 1) * row_size]
}

/// Extract a mutable reference to a specific "row" from a one-dimensional
/// slice, where the data is conceptually arranged as a two-dimensional array.
///
/// # Arguments
///
/// * `slice` - slice to extract a mutable reference from
/// * `index` - the index of the conceptual "row" to reference
/// * `row_size` - the size of the conceptual "row" to reference
#[inline]
pub(crate) fn row_mut<T>(slice: &mut [T], index: usize, row_size: usize) -> &mut [T] {
    &mut slice[index * row_size..(index + 1) * row_size]
}
