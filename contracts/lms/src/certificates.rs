//! Certificate authenticity verification (issue #1039). Starting
//! scaffold — contracts/lms/ did not exist yet in the workspace.

use soroban_sdk::{contracttype, Address, Env, String};

#[derive(Clone)]
#[contracttype]
pub struct CertificateInfo {
    pub exists: bool,
    pub owner: Option<Address>,
    pub course: Option<String>,
    pub completion_date: Option<u64>,
}

/// Verifies a certificate by id and returns its existence, owner,
/// course, and completion date. `lookup` is the storage read for the
/// certificate record, injected so this stays testable without wiring
/// full contract storage yet.
pub fn verify_certificate(
    _env: &Env,
    certificate_id: u64,
    lookup: impl Fn(u64) -> Option<(Address, String, u64)>,
) -> CertificateInfo {
    match lookup(certificate_id) {
        Some((owner, course, completion_date)) => CertificateInfo {
            exists: true,
            owner: Some(owner),
            course: Some(course),
            completion_date: Some(completion_date),
        },
        None => CertificateInfo {
            exists: false,
            owner: None,
            course: None,
            completion_date: None,
        },
    }
}
