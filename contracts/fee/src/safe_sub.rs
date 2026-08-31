use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SafeSubError {
    Underflow = 1,
}

/// Subtracts `b` from `a`, returning an underflow error instead of wrapping when `b` is greater than `a`.
pub fn safe_sub(a: i128, b: i128) -> Result<i128, SafeSubError> {
    if b > a {
        return Err(SafeSubError::Underflow);
    }
    Ok(a - b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_sub() {
        assert_eq!(safe_sub(100, 40), Ok(60));
    }

    #[test]
    fn test_zero_result() {
        assert_eq!(safe_sub(50, 50), Ok(0));
    }

    #[test]
    fn test_underflow() {
        assert_eq!(safe_sub(10, 20), Err(SafeSubError::Underflow));
    }
}
