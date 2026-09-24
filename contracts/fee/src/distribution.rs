//! Splits collected fees across treasury, protocol, and stakeholder shares
//! according to a validated basis-point configuration. Every `pub fn` below
//! already carries a `///` doc comment; this module doc summarizes the file
//! as a whole for `cargo doc`.

#[derive(Debug, Clone)]
pub struct DistributionConfig {
    pub treasury_bps: u16,
    pub protocol_bps: u16,
    pub stakeholder_bps: u16,
}

impl DistributionConfig {
    /// Validates that the configured distribution shares add up to 100%.
    pub fn validate(&self) -> Result<(), &'static str> {
        let total =
            self.treasury_bps +
            self.protocol_bps +
            self.stakeholder_bps;

        if total != 10_000 {
            return Err("distribution percentages must equal 100%");
        }

        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub struct DistributionResult {
    pub treasury: u64,
    pub protocol: u64,
    pub stakeholder: u64,
}

/// Distributes an amount of collected fees across treasury, protocol, and stakeholder shares.
pub fn distribute_fees(
    amount: u64,
    config: &DistributionConfig,
) -> Result<DistributionResult, &'static str> {
    config.validate()?;

    let treasury =
        amount * config.treasury_bps as u64 / 10_000;

    let protocol =
        amount * config.protocol_bps as u64 / 10_000;

    let stakeholder =
        amount - treasury - protocol;

    Ok(DistributionResult {
        treasury,
        protocol,
        stakeholder,
    })
}
