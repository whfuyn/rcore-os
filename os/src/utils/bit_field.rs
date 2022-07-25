// A poor man's bit_field. Imported from whfuyn/fyos.

use core::ops::Bound;
use core::ops::RangeBounds;
use core::ops::{Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive};

/// An abstrction to allow set_bits to work with both the ranges and index.
pub trait IntoSpan {
    fn into_span<T: BitWidth>(self) -> (u8, u8);
}

impl IntoSpan for u8 {
    fn into_span<T: BitWidth>(self) -> (u8, u8) {
        assert!(
            (self as u32) < <T as BitWidth>::BITS,
            "bit index exceed target bit width",
        );
        (self, self)
    }
}

macro_rules! impl_into_span {
    ($ty:ty) => {
        impl IntoSpan for $ty {
            fn into_span<T: BitWidth>(self) -> (u8, u8) {
                from_range::<Self, T>(self)
            }
        }
    };
    ($($ty:ty),*$(,)?) => {
        $(impl_into_span!($ty);)*
    };
}

impl_into_span! {
    Range<u8>, RangeFrom<u8>, RangeFull, RangeTo<u8>, RangeInclusive<u8>, RangeToInclusive<u8>,
}

pub trait BitWidth {
    const BITS: u32;
}

macro_rules! impl_bit_width {
    ($ty:ty) => {
        impl BitWidth for $ty {
            const BITS: u32 = <$ty>::BITS;
        }
    };
    ($($ty:ty),* $(,)?) => {
        $(impl_bit_width!($ty);)*
    };
}

impl_bit_width! {
    u8, u16, u32, u64, usize,
}

/// Turn various types of range into span.
/// # Panic
/// Panics if the range isn't valid or bits exceed the target width.
fn from_range<R: RangeBounds<u8>, T: BitWidth>(range: R) -> (u8, u8) {
    const INVALID_BIT_RANGE: &str = "invalid bit range";

    let start = match range.start_bound() {
        Bound::Included(&i) => i,
        Bound::Excluded(&i) => i.checked_add(1).expect(INVALID_BIT_RANGE),
        Bound::Unbounded => 0,
    };
    let end = match range.end_bound() {
        Bound::Included(&i) => i,
        Bound::Excluded(&i) => i.checked_sub(1).expect(INVALID_BIT_RANGE),
        Bound::Unbounded => (<T as BitWidth>::BITS - 1) as u8,
    };
    assert!(
        start <= end && end < <T as BitWidth>::BITS as u8,
        "{}",
        INVALID_BIT_RANGE
    );
    (start, end)
}

pub trait BitField: Sized {
    fn get_bits<R: IntoSpan>(&self, range: R) -> Self;
    fn set_bits<R: IntoSpan>(&mut self, range: R, bits: Self);
}

macro_rules! impl_bit_field {
    ($ty:ty) => {
        impl BitField for $ty {
            /// Get bit pattern in range.
            /// # Panics
            /// Panics if the range isn't valid
            fn get_bits<R: IntoSpan>(&self, range: R) -> Self {
                let (start, end) = range.into_span::<$ty>();
                // Get a full mask for the range span.
                let mask: $ty = 1u64.checked_shl((end - start + 1) as u32)
                        .map(|r| r - 1)
                        .unwrap_or(u64::MAX) as $ty;
                (*self >> start) & mask
            }

            /// Set self's bit pattern in range to bits.
            /// # Panics
            /// Panics if the range isn't valid or given bits excess the range.
            fn set_bits<R: IntoSpan>(&mut self, range: R, bits: $ty) {
                let (start, end) = range.into_span::<$ty>();
                // Get a full mask for the range span.
                let mask: $ty = 1u64.checked_shl((end - start + 1) as u32)
                        .map(|r| r - 1)
                        .unwrap_or(u64::MAX) as $ty;
                assert!(bits & !mask == 0, "bits fall outside of range");
                // Clear that range and put bits in.
                *self = (*self & !(mask << start)) | bits << start;
            }
        }
    };
    ($($ty:ty),*$(,)?) => {
        $(impl_bit_field!($ty);)*
    };
}

impl_bit_field! {
    u8, u16, u32, u64, usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_bit_field_basic() {
        let mut bits: u16 = 0;
        bits.set_bits(.., 0b111);
        assert_eq!(bits, 0b111);
        bits.set_bits(..=3, 0);
        assert_eq!(bits, 0);
        bits.set_bits(3..=5, 0b101);
        assert_eq!(bits, 0b101000);
        bits.set_bits(4..5, 1);
        assert_eq!(bits, 0b111000);
        bits.set_bits(4.., 0);
        assert_eq!(bits, 0b1000);
        bits.set_bits(2, 1);
        assert_eq!(bits, 0b1100);

        assert_eq!(bits.get_bits(..), 0b1100);
        assert_eq!(bits.get_bits(1..=2), 0b10);
        assert_eq!(bits.get_bits(1), 0);
        assert_eq!(bits.get_bits(2), 1);
    }
}
