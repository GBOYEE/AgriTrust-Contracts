use soroban_sdk::{contracterror, contracttype, Address, Env, Vec, Symbol, symbol_short, Map};

/// Security Council module implementing 3-of-5 multi-sig veto authority
/// over DAO-triggered governance actions with 48-hour timelock.

// --- Constants ---
const TIMELOCK_DURATION_SECS: u64 = 48 * 60 * 60; // 48 hours
const COUNCIL_SIZE: u32 = 5;
const REQUIRED_SIGNATURES: u32 = 3;
const KEY_ROTATION_TIMELOCK_SECS: u64 = 7 * 24 * 60 * 60; // 7 days
const KEY_ROTATION_PERIOD_SECS: u64 = 365 * 24 * 60 * 60; // 1 year

// --- Types ---

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ActionType {
    Clawback,
    EmergencyPause,
    RateChange,
    TreasuryWithdraw,
    AdminChange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ActionStatus {
    Pending,      // Waiting for timelock to expire
    Executed,     // Successfully executed
    Vetoed,       // Blocked by Security Council
    Expired,      // Timelock expired without execution
}

#[derive(Clone)]
#[contracttype]
pub struct PendingAction {
    pub action_id: u64,
    pub action_type: ActionType,
    pub target_grant_id: Option<u64>,
    pub initiator: Address,
    pub created_at: u64,
    pub executable_at: u64,
    pub status: ActionStatus,
    pub parameters: Vec<i128>, // Generic parameters for the action
}

#[derive(Clone)]
#[contracttype]
pub struct CouncilRotation {
    pub proposed_members: Vec<Address>,
    pub proposed_at: u64,
    pub executable_at: u64,
    pub dao_approved: bool,
    pub executed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum StorageKey {
    CouncilMembers,
    PendingAction(u64),
    ActionCounter,
    VetoSignatures(u64), // action_id -> Map<Address, bool>
    LastRotation,
    PendingRotation,
    ActionIds,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SecurityCouncilError {
    NotCouncilMember = 100,
    InsufficientSignatures = 101,
    ActionNotFound = 102,
    ActionAlreadyVetoed = 103,
    ActionAlreadyExecuted = 104,
    TimelockNotExpired = 105,
    ActionExpired = 106,
    InvalidCouncilSize = 107,
    RotationNotReady = 108,
    RotationNotApproved = 109,
    AlreadySigned = 110,
    NotInitialized = 111,
}

// --- Helper Functions ---

fn read_council_members(env: &Env) -> Result<Vec<Address>, SecurityCouncilError> {
    env.storage()
        .instance()
        .get(&StorageKey::CouncilMembers)
        .ok_or(SecurityCouncilError::NotInitialized)
}

fn is_council_member(env: &Env, address: &Address) -> bool {
    if let Ok(members) = read_council_members(env) {
        for i in 0..members.len() {
            if let Some(member) = members.get(i) {
                if member == *address {
                    return true;
                }
            }
        }
    }
    false
}

fn require_council_member(env: &Env, address: &Address) -> Result<(), SecurityCouncilError> {
    if !is_council_member(env, address) {
        return Err(SecurityCouncilError::NotCouncilMember);
    }
    address.require_auth();
    Ok(())
}

fn get_next_action_id(env: &Env) -> u64 {
    let current: u64 = env.storage()
        .instance()
        .get(&StorageKey::ActionCounter)
        .unwrap_or(0);
    let next = current + 1;
    env.storage().instance().set(&StorageKey::ActionCounter, &next);
    next
}

fn read_pending_action(env: &Env, action_id: u64) -> Result<PendingAction, SecurityCouncilError> {
    env.storage()
        .instance()
        .get(&StorageKey::PendingAction(action_id))
        .ok_or(SecurityCouncilError::ActionNotFound)
}

fn write_pending_action(env: &Env, action: &PendingAction) {
    env.storage()
        .instance()
        .set(&StorageKey::PendingAction(action.action_id), action);
}

fn get_veto_signatures(env: &Env, action_id: u64) -> Map<Address, bool> {
    env.storage()
        .instance()
        .get(&StorageKey::VetoSignatures(action_id))
        .unwrap_or_else(|| Map::new(env))
}

fn count_veto_signatures(env: &Env, action_id: u64) -> u32 {
    let signatures = get_veto_signatures(env, action_id);
    let mut count = 0u32;
    let members = read_council_members(env).unwrap_or_else(|_| Vec::new(env));
    
    for i in 0..members.len() {
        if let Some(member) = members.get(i) {
            if signatures.get(member).unwrap_or(false) {
                count += 1;
            }
        }
    }
    count
}

// --- Public Interface ---

impl SecurityCouncil {
    /// Initialize the Security Council with 5 members
    pub fn initialize_council(env: Env, members: Vec<Address>) -> Result<(), SecurityCouncilError> {
        if env.storage().instance().has(&StorageKey::CouncilMembers) {
            return Err(SecurityCouncilError::NotInitialized);
        }
        
        if members.len() != COUNCIL_SIZE {
            return Err(SecurityCouncilError::InvalidCouncilSize);
        }

        env.storage().instance().set(&StorageKey::CouncilMembers, &members);
        env.storage().instance().set(&StorageKey::ActionCounter, &0u64);
        env.storage().instance().set(&StorageKey::LastRotation, &env.ledger().timestamp());
        env.storage().instance().set(&StorageKey::ActionIds, &Vec::<u64>::new(&env));

        env.events().publish(
            (symbol_short!("cncl_init"),),
            members.len(),
        );

        Ok(())
    }

    /// Create a pending governance action with 48-hour timelock
    /// This should be called by the DAO/Admin when initiating sensitive actions
    pub fn create_pending_action(
        env: Env,
        action_type: ActionType,
        target_grant_id: Option<u64>,
        initiator: Address,
        parameters: Vec<i128>,
    ) -> Result<u64, SecurityCouncilError> {
        initiator.require_auth();

        let now = env.ledger().timestamp();
        let action_id = get_next_action_id(&env);

        let action = PendingAction {
            action_id,
            action_type: action_type.clone(),
            target_grant_id,
            initiator: initiator.clone(),
            created_at: now,
            executable_at: now + TIMELOCK_DURATION_SECS,
            status: ActionStatus::Pending,
            parameters,
        };

        write_pending_action(&env, &action);

        // Track action IDs
        let mut action_ids: Vec<u64> = env.storage()
            .instance()
            .get(&StorageKey::ActionIds)
            .unwrap_or_else(|| Vec::new(&env));
        action_ids.push_back(action_id);
        env.storage().instance().set(&StorageKey::ActionIds, &action_ids);

        env.events().publish(
            (symbol_short!("act_pend"), action_type, initiator),
            (action_id, now + TIMELOCK_DURATION_SECS),
        );

        Ok(action_id)
    }

    /// Security Council member signs to veto an action
    /// Requires 3 of 5 signatures to permanently block the action
    pub fn sign_veto(env: Env, action_id: u64, signer: Address) -> Result<(), SecurityCouncilError> {
        require_council_member(&env, &signer)?;

        let mut action = read_pending_action(&env, action_id)?;

        if action.status != ActionStatus::Pending {
            return Err(SecurityCouncilError::ActionAlreadyVetoed);
        }

        // Check if already signed
        let mut signatures = get_veto_signatures(&env, action_id);
        if signatures.get(signer.clone()).unwrap_or(false) {
            return Err(SecurityCouncilError::AlreadySigned);
        }

        // Add signature
        signatures.set(signer.clone(), true);
        env.storage()
            .instance()
            .set(&StorageKey::VetoSignatures(action_id), &signatures);

        let signature_count = count_veto_signatures(&env, action_id);

        env.events().publish(
            (symbol_short!("veto_sign"), signer),
            (action_id, signature_count),
        );

        // If threshold reached, veto the action
        if signature_count >= REQUIRED_SIGNATURES {
            action.status = ActionStatus::Vetoed;
            write_pending_action(&env, &action);

            env.events().publish(
                (symbol_short!("act_veto"), action.action_type, action.initiator),
                (action_id, signature_count),
            );
        }

        Ok(())
    }

    /// Execute a pending action after timelock expires (if not vetoed)
    /// This should be called by the original initiator or admin
    pub fn execute_action(env: Env, action_id: u64) -> Result<(), SecurityCouncilError> {
        let mut action = read_pending_action(&env, action_id)?;

        if action.status == ActionStatus::Vetoed {
            return Err(SecurityCouncilError::ActionAlreadyVetoed);
        }

        if action.status == ActionStatus::Executed {
            return Err(SecurityCouncilError::ActionAlreadyExecuted);
        }

        let now = env.ledger().timestamp();

        if now < action.executable_at {
            return Err(SecurityCouncilError::TimelockNotExpired);
        }

        // Mark as executed
        action.status = ActionStatus::Executed;
        write_pending_action(&env, &action);

        env.events().publish(
            (symbol_short!("act_exec"), action.action_type, action.initiator),
            action_id,
        );

        Ok(())
    }

    /// Check if an action can be executed (timelock expired and not vetoed)
    pub fn can_execute_action(env: Env, action_id: u64) -> Result<bool, SecurityCouncilError> {
        let action = read_pending_action(&env, action_id)?;
        let now = env.ledger().timestamp();

        Ok(action.status == ActionStatus::Pending && now >= action.executable_at)
    }

    /// Propose new council members (requires DAO approval)
    pub fn propose_council_rotation(
        env: Env,
        new_members: Vec<Address>,
        dao_admin: Address,
    ) -> Result<(), SecurityCouncilError> {
        dao_admin.require_auth();

        if new_members.len() != COUNCIL_SIZE {
            return Err(SecurityCouncilError::InvalidCouncilSize);
        }

        let now = env.ledger().timestamp();
        let rotation = CouncilRotation {
            proposed_members: new_members.clone(),
            proposed_at: now,
            executable_at: now + KEY_ROTATION_TIMELOCK_SECS,
            dao_approved: true,
            executed: false,
        };

        env.storage().instance().set(&StorageKey::PendingRotation, &rotation);

        env.events().publish(
            (symbol_short!("rot_prop"), dao_admin),
            now + KEY_ROTATION_TIMELOCK_SECS,
        );

        Ok(())
    }

    /// Execute council rotation after 7-day timelock
    pub fn execute_council_rotation(env: Env) -> Result<(), SecurityCouncilError> {
        let rotation: CouncilRotation = env.storage()
            .instance()
            .get(&StorageKey::PendingRotation)
            .ok_or(SecurityCouncilError::RotationNotReady)?;

        if rotation.executed {
            return Err(SecurityCouncilError::RotationNotReady);
        }

        if !rotation.dao_approved {
            return Err(SecurityCouncilError::RotationNotApproved);
        }

        let now = env.ledger().timestamp();
        if now < rotation.executable_at {
            return Err(SecurityCouncilError::TimelockNotExpired);
        }

        // Update council members
        env.storage()
            .instance()
            .set(&StorageKey::CouncilMembers, &rotation.proposed_members);
        env.storage().instance().set(&StorageKey::LastRotation, &now);

        // Mark rotation as executed
        let mut executed_rotation = rotation.clone();
        executed_rotation.executed = true;
        env.storage()
            .instance()
            .set(&StorageKey::PendingRotation, &executed_rotation);

        env.events().publish(
            (symbol_short!("rot_exec"),),
            now,
        );

        Ok(())
    }

    /// Check if council rotation is due (annual requirement)
    pub fn is_rotation_due(env: Env) -> bool {
        let last_rotation: u64 = env.storage()
            .instance()
            .get(&StorageKey::LastRotation)
            .unwrap_or(0);
        let now = env.ledger().timestamp();
        now >= last_rotation + KEY_ROTATION_PERIOD_SECS
    }

    /// Get current council members
    pub fn get_council_members(env: Env) -> Result<Vec<Address>, SecurityCouncilError> {
        read_council_members(&env)
    }

    /// Get pending action details
    pub fn get_pending_action(env: Env, action_id: u64) -> Result<PendingAction, SecurityCouncilError> {
        read_pending_action(&env, action_id)
    }

    /// Get veto signature count for an action
    pub fn get_veto_count(env: Env, action_id: u64) -> u32 {
        count_veto_signatures(&env, action_id)
    }

    /// Get all pending action IDs
    pub fn get_pending_action_ids(env: Env) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&StorageKey::ActionIds)
            .unwrap_or_else(|| Vec::new(&env))
    }
}

pub struct SecurityCouncil;
