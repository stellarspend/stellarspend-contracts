pub fn calculate_fee(env: &Env, amount: i128, config: &FeeConfig) -> Result<i128, &'static str> {
    let now = env.ledger().timestamp();

    let mut fee_rate = config.default_fee_rate;
    for window in &config.windows {
        if now >= window.start && now <= window.end {
            fee_rate = window.fee_rate;
            break;
        }
    }

    let multiplied = safe_multiply(amount, fee_rate).ok_or("Overflow in multiplication")?;
    let fee = safe_divide(multiplied, 10_000).ok_or("Division error")?;

    if fee < 0 {
        return Err("Underflow detected");
    }

    Ok(fee)
}

pub fn validate_amount(amount: i128) -> Result<(), &'static str> {
    if amount < 0 {
        return Err("Negative amount not allowed");
    }
    if amount > i128::MAX / 10_000 {
        return Err("Amount too large");
    }
    Ok(())
}
