use crate::Error;

/// Validates a spend amount: must be strictly positive.
pub fn validate_amount(amount: i128) -> Result<(), Error> {
    if amount > 0 {
        Ok(())
    } else {
        Err(Error::InvalidAmount)
    }
}

/// Validates a rule's limits: neither the weekly cap nor the ZK-required
/// threshold may be negative.
pub fn validate_rule(weekly_limit: i128, zk_required_above: i128) -> Result<(), Error> {
    if weekly_limit < 0 || zk_required_above < 0 {
        Err(Error::InvalidAmount)
    } else {
        Ok(())
    }
}
