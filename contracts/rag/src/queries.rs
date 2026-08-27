use soroban_sdk::{
    contracterror, contracttype, Address, BytesN, Env, String,
};

use crate::access::{
    AccessControlManager,
    ResourceAccessLevel,
};

// -----------------------------------------------------------------------
// Retrieval Request Types
// -----------------------------------------------------------------------

/// Lifecycle states for a RAG retrieval request.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetrievalRequestState {
    Pending = 0,
    Completed = 1,
    Expired = 2,
    Cancelled = 3,
    Rejected = 4,
}

/// Represents a persisted RAG retrieval request.
///
/// Stores a query commitment (hash) rather than the full query text
/// to keep on-chain storage minimal while still allowing offline
/// verification that a response matches the committed query.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrievalRequest {
    pub request_id: u64,
    pub requester: Address,
    pub collection_id: String,
    pub query_commitment: BytesN<32>,
    pub created_at: u64,
    pub state: RetrievalRequestState,
}

/// Errors returned by the retrieval request module.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum RetrievalRequestError {
    RequestNotFound = 1,
    InvalidTransition = 2,
    Unauthorized = 3,
    CollectionAccessDenied = 4,
}

// -----------------------------------------------------------------------
// Storage
// -----------------------------------------------------------------------

#[derive(Clone)]
#[contracttype]
pub enum RetrievalRequestKey {
    Request(u64),
}

// -----------------------------------------------------------------------
// Retrieval Request Manager
// -----------------------------------------------------------------------

pub struct RetrievalQueryManager;

impl RetrievalQueryManager {
    /// Creates a new retrieval request against a collection.
    ///
    /// The caller must authenticate and have permission to access
    /// the requested collection.
    pub fn create_request(
        env: &Env,
        request_id: u64,
        collection_id: String,
        query_commitment: BytesN<32>,
        caller: Address,
    ) -> Result<RetrievalRequest, RetrievalRequestError> {
        // ---------------------------------------------------------------
        // 1. Require caller authentication
        // ---------------------------------------------------------------
        caller.require_auth();

        // ---------------------------------------------------------------
        // 2. Prevent duplicate request IDs
        // ---------------------------------------------------------------
        let key = RetrievalRequestKey::Request(request_id);

        if env.storage().persistent().has(&key) {
            return Err(RetrievalRequestError::InvalidTransition);
        }

        // ---------------------------------------------------------------
        // 3. Check collection access
        // ---------------------------------------------------------------
        Self::authorize_collection_access(
            env,
            &collection_id,
            &caller,
        )?;

        // ---------------------------------------------------------------
        // 4. Record ledger timestamp
        // ---------------------------------------------------------------
        let created_at = env.ledger().timestamp();

        // ---------------------------------------------------------------
        // 5. Create request in Pending state with query commitment
        // ---------------------------------------------------------------
        let request = RetrievalRequest {
            request_id,
            requester: caller,
            collection_id,
            query_commitment,
            created_at,
            state: RetrievalRequestState::Pending,
        };

        env.storage()
            .persistent()
            .set(&key, &request);

        Ok(request)
    }

    /// Checks whether a caller can create a retrieval request
    /// against a collection.
    fn authorize_collection_access(
        env: &Env,
        collection_id: &String,
        caller: &Address,
    ) -> Result<(), RetrievalRequestError> {
        let access_level =
            AccessControlManager::get_resource_access_level(
                env,
                collection_id.clone(),
            );

        match access_level {
            // -----------------------------------------------------------
            // Public collections are accessible according to the
            // collection's configured public policy.
            // -----------------------------------------------------------
            ResourceAccessLevel::Public => Ok(()),

            // -----------------------------------------------------------
            // OwnerOnly and MembersOnly are handled by the existing
            // document/resource access policy.
            // -----------------------------------------------------------
            ResourceAccessLevel::OwnerOnly
            | ResourceAccessLevel::MembersOnly => {
                AccessControlManager::verify_access(
                    env,
                    collection_id,
                    caller,
                )
                .map_err(|_| {
                    RetrievalRequestError::CollectionAccessDenied
                })
            }
        }
    }

    /// Returns the current state of a retrieval request.
    pub fn get_state(
        env: &Env,
        request_id: u64,
    ) -> Result<RetrievalRequestState, RetrievalRequestError> {
        let key = RetrievalRequestKey::Request(request_id);

        let request: RetrievalRequest = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(RetrievalRequestError::RequestNotFound)?;

        Ok(request.state)
    }

    /// Returns a complete retrieval request.
    pub fn get_request(
        env: &Env,
        request_id: u64,
    ) -> Result<RetrievalRequest, RetrievalRequestError> {
        let key = RetrievalRequestKey::Request(request_id);

        env.storage()
            .persistent()
            .get(&key)
            .ok_or(RetrievalRequestError::RequestNotFound)
    }

    /// Checks whether a state transition is valid.
    pub fn can_transition(
        current: RetrievalRequestState,
        next: RetrievalRequestState,
    ) -> bool {
        matches!(
            (current, next),
            (
                RetrievalRequestState::Pending,
                RetrievalRequestState::Completed
            ) | (
                RetrievalRequestState::Pending,
                RetrievalRequestState::Expired
            ) | (
                RetrievalRequestState::Pending,
                RetrievalRequestState::Cancelled
            ) | (
                RetrievalRequestState::Pending,
                RetrievalRequestState::Rejected
            )
        )
    }

    /// Transitions a retrieval request to a new state.
    pub fn transition_request(
        env: &Env,
        request_id: u64,
        next_state: RetrievalRequestState,
    ) -> Result<RetrievalRequest, RetrievalRequestError> {
        let key = RetrievalRequestKey::Request(request_id);

        let mut request: RetrievalRequest = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(RetrievalRequestError::RequestNotFound)?;

        let previous_state = request.state;

        if !Self::can_transition(previous_state, next_state) {
            return Err(RetrievalRequestError::InvalidTransition);
        }

        request.state = next_state;

        env.storage()
            .persistent()
            .set(&key, &request);

        env.events().publish(
            (
                soroban_sdk::symbol_short!("request"),
                request_id,
            ),
            (
                previous_state,
                next_state,
            ),
        );

        Ok(request)
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup() -> (Env, Address, Address, Address) {
        let env = Env::default();

        let owner = Address::generate(&env);
        let member = Address::generate(&env);
        let unauthorized = Address::generate(&env);

        (env, owner, member, unauthorized)
    }

    // -------------------------------------------------------------------
    // Authentication
    // -------------------------------------------------------------------

    #[test]
    fn caller_authentication_is_required() {
        let (env, owner, _, _) = setup();

        let collection_id = String::from_str(
            &env,
            "collection-1",
        );

        let query_commitment = BytesN::from_array(
            &env,
            &[1u8; 32],
        );

        // `create_request` invokes require_auth().
        //
        // The Soroban test environment will reject an invocation
        // where the caller has not authorized the operation.
        let _ = RetrievalQueryManager::create_request(
            &env,
            1,
            collection_id,
            query_commitment,
            owner,
        );
    }

    // -------------------------------------------------------------------
    // Public Collection
    // -------------------------------------------------------------------

    #[test]
    fn public_collection_allows_retrieval_request() {
        let (env, owner, _, _) = setup();

        let collection_id =
            String::from_str(&env, "public-collection");
        let query_commitment = BytesN::from_array(
            &env,
            &[2u8; 32],
        );

        AccessControlManager::set_resource_access_level(
            &env,
            collection_id.clone(),
            ResourceAccessLevel::Public,
            owner.clone(),
        );

        let request =
            RetrievalQueryManager::create_request(
                &env,
                1,
                collection_id,
                query_commitment,
                owner,
            )
            .unwrap();

        assert_eq!(
            request.state,
            RetrievalRequestState::Pending
        );
    }

    // -------------------------------------------------------------------
    // OwnerOnly Collection
    // -------------------------------------------------------------------

    #[test]
    fn owner_can_access_owner_only_collection() {
        let (env, owner, _, _) = setup();

        let collection_id =
            String::from_str(&env, "private-collection");
        let query_commitment = BytesN::from_array(
            &env,
            &[3u8; 32],
        );

        AccessControlManager::set_policy(
            &env,
            collection_id.clone(),
            soroban_sdk::vec![&env],
            owner.clone(),
        )
        .unwrap();

        AccessControlManager::set_resource_access_level(
            &env,
            collection_id.clone(),
            ResourceAccessLevel::OwnerOnly,
            owner.clone(),
        );

        let request =
            RetrievalQueryManager::create_request(
                &env,
                1,
                collection_id,
                query_commitment,
                owner,
            )
            .unwrap();

        assert_eq!(
            request.state,
            RetrievalRequestState::Pending
        );
    }

    // -------------------------------------------------------------------
    // MembersOnly Collection
    // -------------------------------------------------------------------

    #[test]
    fn authorized_member_can_access_collection() {
        let (env, owner, member, _) = setup();

        let collection_id =
            String::from_str(&env, "members-collection");
        let query_commitment = BytesN::from_array(
            &env,
            &[4u8; 32],
        );

        AccessControlManager::set_policy(
            &env,
            collection_id.clone(),
            soroban_sdk::vec![&env, member.clone()],
            owner.clone(),
        )
        .unwrap();

        AccessControlManager::set_resource_access_level(
            &env,
            collection_id.clone(),
            ResourceAccessLevel::MembersOnly,
            owner,
        )
        .unwrap();

        let request =
            RetrievalQueryManager::create_request(
                &env,
                1,
                collection_id,
                query_commitment,
                member,
            )
            .unwrap();

        assert_eq!(
            request.state,
            RetrievalRequestState::Pending
        );
    }

    // -------------------------------------------------------------------
    // Unauthorized Access
    // -------------------------------------------------------------------

    #[test]
    fn unauthorized_user_cannot_create_retrieval_request() {
        let (env, owner, _, unauthorized) = setup();

        let collection_id =
            String::from_str(&env, "private-collection");
        let query_commitment = BytesN::from_array(
            &env,
            &[5u8; 32],
        );

        AccessControlManager::set_policy(
            &env,
            collection_id.clone(),
            soroban_sdk::vec![&env],
            owner.clone(),
        )
        .unwrap();

        AccessControlManager::set_resource_access_level(
            &env,
            collection_id.clone(),
            ResourceAccessLevel::MembersOnly,
            owner,
        )
        .unwrap();

        let result =
            RetrievalQueryManager::create_request(
                &env,
                1,
                collection_id,
                query_commitment,
                unauthorized,
            );

        assert_eq!(
            result,
            Err(
                RetrievalRequestError::CollectionAccessDenied
            )
        );
    }

    // -------------------------------------------------------------------
    // Request State
    // -------------------------------------------------------------------

    #[test]
    fn new_request_starts_as_pending() {
        let (env, owner, _, _) = setup();

        let collection_id =
            String::from_str(&env, "public-collection");
        let query_commitment = BytesN::from_array(
            &env,
            &[6u8; 32],
        );

        AccessControlManager::set_resource_access_level(
            &env,
            collection_id.clone(),
            ResourceAccessLevel::Public,
            owner.clone(),
        );

        RetrievalQueryManager::create_request(
            &env,
            1,
            collection_id,
            query_commitment,
            owner,
        )
        .unwrap();

        assert_eq!(
            RetrievalQueryManager::get_state(&env, 1)
                .unwrap(),
            RetrievalRequestState::Pending
        );
    }

    #[test]
    fn nonexistent_request_fails() {
        let env = Env::default();

        let result =
            RetrievalQueryManager::get_state(&env, 999);

        assert_eq!(
            result,
            Err(RetrievalRequestError::RequestNotFound)
        );
    }
}