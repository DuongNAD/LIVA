//! Safe signed 16-bit packed LPARAM bitwise conversion.
//!
//! Win32 mouse window messages (`WM_MOUSEMOVE`, `WM_LBUTTONDOWN`, `WM_LBUTTONUP`, etc.)
//! pack the client coordinates into an `LPARAM`:
//! - Bits 0..15 (`LOWORD`): x-coordinate
//! - Bits 16..31 (`HIWORD`): y-coordinate
//!
//! Windows macros `GET_X_LPARAM` / `GET_Y_LPARAM` sign-extend each half via `(int)(short)`.
//! Coordinates must be within `[-32768, 32767]` (`i16`) range to prevent silent bit truncation.

use thiserror::Error;

#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LParamPackError {
    #[error("LPARAM pack error: x-coordinate {0} is outside valid i16 range [-32768, 32767]")]
    XOutOfRange(i32),
    #[error("LPARAM pack error: y-coordinate {0} is outside valid i16 range [-32768, 32767]")]
    YOutOfRange(i32),
}

/// Pack client-area coordinates `(x, y)` into a 32-bit integer payload.
///
/// Preserves signed two's-complement bit patterns so `GET_X_LPARAM` / `GET_Y_LPARAM`
/// recover negative values without sign inversion.
#[inline]
pub fn pack_xy(x: i32, y: i32) -> Result<u32, LParamPackError> {
    if !fits_i16(x) {
        return Err(LParamPackError::XOutOfRange(x));
    }
    if !fits_i16(y) {
        return Err(LParamPackError::YOutOfRange(y));
    }

    let low = (x as i16 as u16) as u32;
    let high = (y as i16 as u16) as u32;
    Ok((high << 16) | low)
}

/// Pack client-area coordinates `(x, y)` directly into a Win32 `LPARAM` (`isize`).
#[inline]
pub fn pack_lparam(x: i32, y: i32) -> Result<isize, LParamPackError> {
    pack_xy(x, y).map(|packed| packed as usize as isize)
}

/// Saturating variant: clamps `x` and `y` into `i16::MIN..=i16::MAX` before packing.
#[inline]
pub fn pack_lparam_clamped(x: i32, y: i32) -> isize {
    let clamped_x = x.clamp(i16::MIN as i32, i16::MAX as i32);
    let clamped_y = y.clamp(i16::MIN as i32, i16::MAX as i32);
    pack_lparam(clamped_x, clamped_y).unwrap_or(0)
}

/// Unpack signed x-coordinate from an `LPARAM` (equivalent to Win32 `GET_X_LPARAM`).
#[inline]
pub fn unpack_x(lparam: isize) -> i32 {
    (lparam as usize & 0xFFFF) as u16 as i16 as i32
}

/// Unpack signed y-coordinate from an `LPARAM` (equivalent to Win32 `GET_Y_LPARAM`).
#[inline]
pub fn unpack_y(lparam: isize) -> i32 {
    ((lparam as usize >> 16) & 0xFFFF) as u16 as i16 as i32
}

/// Unpack both `(x, y)` coordinates from an `LPARAM`.
#[inline]
pub fn unpack_xy(lparam: isize) -> (i32, i32) {
    (unpack_x(lparam), unpack_y(lparam))
}

#[inline]
fn fits_i16(val: i32) -> bool {
    (i16::MIN as i32..=i16::MAX as i32).contains(&val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_positive_and_negative_round_trip() {
        let cases = [
            (0, 0),
            (100, 200),
            (-50, -100),
            (-1795, 383),
            (-1198, 292),
            (i16::MAX as i32, i16::MAX as i32),
            (i16::MIN as i32, i16::MIN as i32),
            (i16::MIN as i32, i16::MAX as i32),
        ];

        for &(x, y) in &cases {
            let lp = pack_lparam(x, y).expect("coordinate fits in i16");
            let (rx, ry) = unpack_xy(lp);
            assert_eq!((rx, ry), (x, y), "round-trip mismatch for ({}, {})", x, y);
        }
    }

    #[test]
    fn test_out_of_range_rejection() {
        assert!(matches!(
            pack_xy(i16::MAX as i32 + 1, 0),
            Err(LParamPackError::XOutOfRange(32768))
        ));
        assert!(matches!(
            pack_xy(0, i16::MIN as i32 - 1),
            Err(LParamPackError::YOutOfRange(-32769))
        ));
    }

    #[test]
    fn test_clamped_packing() {
        let lp = pack_lparam_clamped(50000, -50000);
        let (rx, ry) = unpack_xy(lp);
        assert_eq!(rx, i16::MAX as i32);
        assert_eq!(ry, i16::MIN as i32);
    }
}
