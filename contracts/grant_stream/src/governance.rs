#![no_std]

use soroban_sdk::{
    contracterror,
    contracttype,
    symbol_short,
    token,
    Address,
    Bytes,
    Env,
    Vec,
    Map,
};
use crate::storage_keys::{StorageKey, VoteKey};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ProposalStatus {
    Active,
    Passed,
    Rejected,
    Executed,
    Optimistic,
    Challenged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ProposalType {
    General,
    SetManualExchangeRate(i128),
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub title: soroban_sdk::String,
    pub description: soroban_sdk::String,
    pub voting_deadline: u64,
    pub status: ProposalStatus,
    pub yes_votes: i128,
    pub no_votes: i128,
    pub total_voting_power: i128,
    pub stake_amount: i128,
    pub stake_returned: bool,
    pub is_optimistic: bool,
    pub challenge_deadline: u64,
    pub proposal_type: ProposalType,
    pub created_at: u64,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct Vote {
    pub voter: Address,
    pub proposal_id: u64,
    pub weight: i128,
    pub voting_power: i128,
    pub conviction: i128,
    pub voted_at: u64,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct VotingPower {
    pub address: Address,
    pub token_balance: i128,
    pub voting_power: i128,
    pub conviction: i128,
    pub last_updated: u64,
}

// Legacy GovernanceDataKey type alias preserved for backward compatibility.
// All runtime storage uses `StorageKey` directly.
type GovernanceDataKey = StorageKey;

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum GovernanceError {
    NotInitialized = 101,
    AlreadyInitialized = 102,
    NotAuthorized = 103,
    ProposalNotFound = 104,
    ProposalAlreadyExists = 105,
    VotingEnded = 106,
    InvalidWeight = 107,
    InvalidAmount = 108,
    MathOverflow = 109,
    QuorumNotMet = 110,
    ThresholdNotMet = 111,
    AlreadyVoted = 112,
    NotCouncilMember = 113,
    InsufficientStake = 113,
    StakeAlreadyReturned = 114,
    ProposalNotConcluded = 115,
    InvalidOptimisticAmount = 116,
    ChallengeWindowClosed = 117,
    NotOptimistic = 118,
}

pub struct GovernanceContract;

// ---------------------------------------------------------------------------
// Council auth helpers — the core of this optimization
// ---------------------------------------------------------------------------

/// Convert an `Address` to its canonical XDR byte representation.
/// Called once per auth check, outside any loop.
fn addr_to_bytes(env: &Env, addr: &Address) -> Bytes {
    addr.to_xdr(env)
}

/// Check membership using raw byte comparison.
///
/// # Why this is faster than a `Vec<Address>` loop
/// Comparing `Bytes` values is a simple length-then-memcmp operation on the
/// host side.  The naive alternative — storing `Vec<Address>` and calling
/// `==` on each element — forces the host to deserialize each stored address
/// into a full `ScAddress` object before the comparison, costing additional
/// allocations and CPU instructions on every iteration.
///
/// By storing pre-serialized bytes and converting the *caller* to bytes once
/// before the loop, we pay the serialization cost exactly once regardless of
/// council size.
fn is_council_member(env: &Env, caller_bytes: &Bytes) -> bool {
    let members: Vec<Bytes> = env
        .storage()
        .instance()
        .get(&GovernanceDataKey::CouncilMembers)
        .unwrap_or_else(|| Vec::new(env));

    // Raw byte comparison — no Address object construction inside the loop.
    for member_bytes in members.iter() {
        if member_bytes == *caller_bytes {
            return true;
        }
    }
    false
}

/// Require that `caller` is a registered council member.
/// Converts `caller` to bytes once, then delegates to `is_council_member`.
fn require_council_auth(env: &Env, caller: &Address) -> Result<(), GovernanceError> {
    caller.require_auth();
    let caller_bytes = addr_to_bytes(env, caller);
    if !is_council_member(env, &caller_bytes) {
        return Err(GovernanceError::NotCouncilMember);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Contract implementation
// ---------------------------------------------------------------------------

impl GovernanceContract {
    pub fn initialize(
        env: Env,
        governance_token: Address,
        voting_threshold: i128,
        quorum_threshold: i128,
        stake_token: Address,
        proposal_stake_amount: i128
    ) -> Result<(), GovernanceError> {
        if env.storage().instance().has(&GovernanceDataKey::GovernanceToken) {
            return Err(GovernanceError::AlreadyInitialized);
        }

        env.storage().instance().set(&GovernanceDataKey::GovernanceToken, &governance_token);
        env.storage().instance().set(&GovernanceDataKey::VotingThreshold, &voting_threshold);
        env.storage().instance().set(&GovernanceDataKey::QuorumThreshold, &quorum_threshold);
        env.storage().instance().set(&GovernanceDataKey::ProposalIds, &Vec::<u64>::new(&env));
        env.storage().instance().set(&GovernanceDataKey::CouncilMembers, &Vec::<Bytes>::new(&env));
        env.storage().instance().set(&GovernanceDataKey::OptimisticLimit, &500_i128); // Default $500
        env.storage().instance().set(&GovernanceDataKey::ChallengeBond, &100_i128); // Default $100
        env.storage().instance().set(&GovernanceDataKey::ConvictionAlpha, &9000_i128); // 0.9

        Ok(())
    }

    /// Replace the full council member list.
    ///
    /// Each `Address` in `members` is serialised to XDR bytes at write time so
    /// that future membership checks never pay that cost again.
    pub fn set_council_members(
        env: Env,
        caller: Address,
        members: Vec<Address>,
    ) -> Result<(), GovernanceError> {
        // Only an existing council member (or the first setup where list is
        // empty) may update the council.
        let existing: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&GovernanceDataKey::CouncilMembers)
            .unwrap_or_else(|| Vec::new(&env));

        if !existing.is_empty() {
            require_council_auth(&env, &caller)?;
        } else {
            caller.require_auth();
        }

        // Serialise once at write time — reads pay zero per-address cost.
        let mut member_bytes: Vec<Bytes> = Vec::new(&env);
        for addr in members.iter() {
            member_bytes.push_back(addr_to_bytes(&env, &addr));
        }

        env.storage().instance().set(&GovernanceDataKey::CouncilMembers, &member_bytes);

        env.events().publish(
            (symbol_short!("cncl_set"),),
            member_bytes.len(),
        );

        Ok(())
    }

    pub fn create_proposal(
        env: Env,
        proposer: Address,
        title: soroban_sdk::String,
        description: soroban_sdk::String,
        voting_period: u64,
    ) -> Result<u64, GovernanceError> {
        Self::propose_grant(env, proposer, title, description, voting_period)
    }

    pub fn propose_grant(
        env: Env,
        proposer: Address,
        title: soroban_sdk::String,
        description: soroban_sdk::String,
        voting_period: u64
    ) -> Result<u64, GovernanceError> {
        proposer.require_auth();

        let stake_token = Self::get_stake_token(&env)?;
        let stake_amount = Self::get_proposal_stake_amount(&env)?;

        // Transfer stake from proposer to this contract
        let token_client = token::Client::new(&env, &stake_token);
        token_client.transfer(&proposer, &env.current_contract_address(), &stake_amount);

        let now = env.ledger().timestamp();
        let voting_deadline = now
            .checked_add(voting_period)
            .ok_or(GovernanceError::MathOverflow)?;

        let mut proposal_ids = Self::get_proposal_ids(&env)?;
        let proposal_id = if proposal_ids.is_empty() {
            0
        } else {
            let last_id = proposal_ids.get(proposal_ids.len() - 1).unwrap();
            last_id.checked_add(1).ok_or(GovernanceError::MathOverflow)?
        };

        let proposal = Proposal {
            id: proposal_id,
            proposer: proposer.clone(),
            title,
            description,
            voting_deadline,
            status: ProposalStatus::Active,
            yes_votes: 0,
            no_votes: 0,
            total_voting_power: 0,
            stake_amount,
            stake_returned: false,
            is_optimistic: false,
            challenge_deadline: 0,
            proposal_type: ProposalType::General,
        };

        env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);
        proposal_ids.push_back(proposal_id);
        env.storage().instance().set(&GovernanceDataKey::ProposalIds, &proposal_ids);

        env.events().publish(
            (symbol_short!("prop_new"), proposal_id, proposer.clone()),
            (proposer, voting_deadline),
        );

        Ok(proposal_id)
    }

    pub fn propose_optimistic_grant(
        env: Env,
        proposer: Address,
        title: soroban_sdk::String,
        description: soroban_sdk::String,
        amount: i128,
    ) -> Result<u64, GovernanceError> {
        proposer.require_auth();

        let limit = env.storage().instance().get::<_, i128>(&GovernanceDataKey::OptimisticLimit).unwrap_or(500);
        if amount > limit {
            return Err(GovernanceError::InvalidOptimisticAmount);
        }

        let stake_token = Self::get_stake_token(&env)?;
        let stake_amount = Self::get_proposal_stake_amount(&env)?;
        let token_client = token::Client::new(&env, &stake_token);
        token_client.transfer(&proposer, &env.current_contract_address(), &stake_amount);

        let now = env.ledger().timestamp();
        let challenge_deadline = now + (48 * 3600); // 48 hours

        let mut proposal_ids = Self::get_proposal_ids(&env)?;
        let proposal_id = if proposal_ids.is_empty() { 0 } else { proposal_ids.get(proposal_ids.len() - 1).unwrap() + 1 };

        let proposal = Proposal {
            id: proposal_id,
            proposer: proposer.clone(),
            title,
            description,
            voting_deadline: challenge_deadline,
            status: ProposalStatus::Optimistic,
            yes_votes: 0,
            no_votes: 0,
            total_voting_power: 0,
            stake_amount,
            stake_returned: false,
            is_optimistic: true,
            challenge_deadline,
            proposal_type: ProposalType::General,
            created_at: now,
        };

        env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);
        proposal_ids.push_back(proposal_id);
        env.storage().instance().set(&GovernanceDataKey::ProposalIds, &proposal_ids);

        env.events().publish(
            (symbol_short!("opti_new"), proposal_id, proposer),
            (amount, challenge_deadline),
        );

        Ok(proposal_id)
    }

    pub fn create_set_manual_exchange_rate_proposal(
        env: Env,
        proposer: Address,
        rate: i128,
        voting_period: u64,
    ) -> Result<u64, GovernanceError> {
        proposer.require_auth();

        let stake_token = Self::get_stake_token(&env)?;
        let stake_amount = Self::get_proposal_stake_amount(&env)?;
        let token_client = token::Client::new(&env, &stake_token);
        token_client.transfer(&proposer, &env.current_contract_address(), &stake_amount);

        let now = env.ledger().timestamp();
        let voting_deadline = now.checked_add(voting_period).ok_or(GovernanceError::MathOverflow)?;

        let mut proposal_ids = Self::get_proposal_ids(&env)?;
        let proposal_id = if proposal_ids.is_empty() { 0 } else { proposal_ids.get(proposal_ids.len() - 1).unwrap() + 1 };

        let title = String::from_str(&env, "Set Manual Exchange Rate");
        let description = String::from_str(&env, &format!("Set manual exchange rate to {}", rate));

        let proposal = Proposal {
            id: proposal_id,
            proposer: proposer.clone(),
            title,
            description,
            voting_deadline,
            status: ProposalStatus::Active,
            yes_votes: 0,
            no_votes: 0,
            total_voting_power: 0,
            stake_amount,
            stake_returned: false,
            is_optimistic: false,
            challenge_deadline: 0,
            proposal_type: ProposalType::SetManualExchangeRate(rate),
            created_at: now,
        };

        env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);
        proposal_ids.push_back(proposal_id);
        env.storage().instance().set(&GovernanceDataKey::ProposalIds, &proposal_ids);

        env.events().publish(
            (symbol_short!("prop_new"), proposal_id, proposer.clone()),
            (proposer, voting_deadline),
        );

        Ok(proposal_id)
    }

    pub fn challenge_optimistic_grant(
        env: Env,
        challenger: Address,
        proposal_id: u64,
    ) -> Result<(), GovernanceError> {
        challenger.require_auth();

        let mut proposal = Self::get_proposal(&env, proposal_id)?;
        if !proposal.is_optimistic || proposal.status != ProposalStatus::Optimistic {
            return Err(GovernanceError::NotOptimistic);
        }

        let now = env.ledger().timestamp();
        if now >= proposal.challenge_deadline {
            return Err(GovernanceError::ChallengeWindowClosed);
        }

        let bond = env.storage().instance().get::<_, i128>(&GovernanceDataKey::ChallengeBond).unwrap_or(100);
        let stake_token = Self::get_stake_token(&env)?;
        let token_client = token::Client::new(&env, &stake_token);
        token_client.transfer(&challenger, &env.current_contract_address(), &bond);

        proposal.status = ProposalStatus::Challenged;
        env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (symbol_short!("opti_chal"), proposal_id, challenger),
            now,
        );

        Ok(())
    }

    pub fn quadratic_vote(
        env: Env,
        voter: Address,
        proposal_id: u64,
        weight: i128,
    ) -> Result<(), GovernanceError> {
        voter.require_auth();

        if weight <= 0 {
            return Err(GovernanceError::InvalidWeight);
        }

        let mut proposal = Self::get_proposal(&env, proposal_id)?;
        let now = env.ledger().timestamp();

        if now >= proposal.voting_deadline {
            return Err(GovernanceError::VotingEnded);
        }

        if proposal.status != ProposalStatus::Active {
            return Err(GovernanceError::VotingEnded);
        }

        let voting_power = Self::calculate_voting_power(&env, &voter)?;
        let mut conviction_record = env.storage().instance().get::<_, VotingPower>(&GovernanceDataKey::VotingPower(voter.clone())).unwrap_or(VotingPower {
            address: voter.clone(),
            token_balance: 0,
            voting_power,
            conviction: 0,
            last_updated: now,
        });

        // Decay existing conviction: conviction = conviction * alpha / 10000
        let alpha = env.storage().instance().get::<_, i128>(&GovernanceDataKey::ConvictionAlpha).unwrap_or(9000);
        let decayed_conviction = conviction_record.conviction.checked_mul(alpha).unwrap_or(0) / 10000;
        
        // Add new weight to conviction
        let current_conviction = decayed_conviction.checked_add(weight).unwrap_or(weight);
        conviction_record.conviction = current_conviction;
        conviction_record.last_updated = now;
        
        env.storage().instance().set(&GovernanceDataKey::VotingPower(voter.clone()), &conviction_record);

        let vote = Vote {
            voter: voter.clone(),
            proposal_id,
            weight,
            voting_power,
            conviction: current_conviction,
            voted_at: now,
        };

        env.storage()
            .instance()
            .set(&GovernanceDataKey::Vote(VoteKey(voter.clone(), proposal_id)), &vote);

        let conviction_weight = current_conviction
            .checked_mul(weight)
            .ok_or(GovernanceError::MathOverflow)?;

        proposal.yes_votes = proposal.yes_votes
            .checked_add(conviction_weight)
            .ok_or(GovernanceError::MathOverflow)?;

        proposal.total_voting_power = proposal.total_voting_power
            .checked_add(voting_power)
            .ok_or(GovernanceError::MathOverflow)?;

        env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (symbol_short!("quad_vote"), proposal_id),
            (voter, weight, voting_power, conviction_weight),
        );

        Ok(())
    }

    pub fn calculate_voting_power(env: &Env, address: &Address) -> Result<i128, GovernanceError> {
        let governance_token = Self::get_governance_token(env)?;
        let token_client = token::Client::new(env, &governance_token);
        let token_balance = token_client.balance(address);

        let voting_power = Self::integer_sqrt(token_balance);

        let vp_record = VotingPower {
            address: address.clone(),
            token_balance,
            voting_power,
            conviction: 0, // Default to 0 when recalculating power
            last_updated: env.ledger().timestamp(),
        };

        env.storage()
            .instance()
            .set(&GovernanceDataKey::VotingPower(address.clone()), &vp_record);

        Ok(voting_power)
    }

    fn integer_sqrt(n: i128) -> i128 {
        if n <= 0 {
            return 0;
        }
        let mut x = n;
        let mut y = (x + 1) / 2;
        while y < x {
            x = y;
            y = (x + n / x) / 2;
        }
        x
    }

    /// Execute a proposal — requires caller to be a council member.
    ///
    /// The council membership check uses the optimized byte-comparison path:
    /// `caller` is serialised to XDR bytes exactly once, then compared
    /// against the pre-serialised `Vec<Bytes>` in storage.
    pub fn execute_proposal(
        env: Env,
        caller: Address,
        proposal_id: u64,
    ) -> Result<(), GovernanceError> {
        // Single serialisation before the loop inside require_council_auth.
        require_council_auth(&env, &caller)?;

        let mut proposal = Self::get_proposal(&env, proposal_id)?;
        let now = env.ledger().timestamp();

        if now < proposal.voting_deadline {
            return Err(GovernanceError::VotingEnded);
        }

        if proposal.status != ProposalStatus::Active {
            return Err(GovernanceError::VotingEnded);
        }

        let quorum_threshold = Self::get_quorum_threshold(&env)?;
        let voting_threshold = Self::get_voting_threshold(&env)?;

        if proposal.total_voting_power < quorum_threshold {
            proposal.status = ProposalStatus::Rejected;
            env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);
            return Err(GovernanceError::QuorumNotMet);
        }

        let total_votes = proposal.yes_votes.checked_add(proposal.no_votes).unwrap_or(0);
        let required_threshold = match proposal.proposal_type {
            ProposalType::SetManualExchangeRate(_) => {
                // 90% majority required
                proposal.total_voting_power * 90 / 100
            }
            _ => voting_threshold,
        };
        if total_votes == 0 || proposal.yes_votes < required_threshold {
            proposal.status = ProposalStatus::Rejected;
            env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);
            return Err(GovernanceError::ThresholdNotMet);
        }

        proposal.status = ProposalStatus::Executed;
        env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

        // Execute the proposal action
        match proposal.proposal_type {
            ProposalType::SetManualExchangeRate(rate) => {
                super::circuit_breakers::set_manual_exchange_rate(&env, rate);
            }
            _ => {}
        }

        env.events().publish(
            (symbol_short!("prop_exec"), proposal_id),
            (proposal.yes_votes, proposal.no_votes),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // View functions
    // -----------------------------------------------------------------------

    pub fn get_proposal_info(env: Env, proposal_id: u64) -> Result<Proposal, GovernanceError> {
        Self::get_proposal(&env, proposal_id)
    }

    pub fn get_voter_power(env: Env, voter: Address) -> Result<i128, GovernanceError> {
        Self::calculate_voting_power(&env, &voter)
    }

    pub fn get_vote_info(
        env: Env,
        voter: Address,
        proposal_id: u64,
    ) -> Result<Vote, GovernanceError> {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::Vote(VoteKey(voter, proposal_id)))
            .ok_or(GovernanceError::ProposalNotFound)
    }

    /// Expose the raw council bytes for off-chain tooling / auditing.
    pub fn get_council_members(env: Env) -> Vec<Bytes> {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::CouncilMembers)
            .unwrap_or_else(|| Vec::new(&env))
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    pub fn refund_stake(env: Env, proposal_id: u64) -> Result<(), GovernanceError> {
        let mut proposal = Self::get_proposal(&env, proposal_id)?;
        
        if proposal.status != ProposalStatus::Executed && proposal.status != ProposalStatus::Rejected {
            return Err(GovernanceError::ProposalNotConcluded);
        }

        if proposal.stake_returned {
            return Err(GovernanceError::StakeAlreadyReturned);
        }

        proposal.stake_returned = true;
        env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

        if proposal.stake_amount > 0 {
            let stake_token = Self::get_stake_token(&env)?;
            let token_client = token::Client::new(&env, &stake_token);
            token_client.transfer(
                &env.current_contract_address(),
                &proposal.proposer,
                &proposal.stake_amount
            );
        }

        Ok(())
    }

    pub fn slash_stake(env: Env, admin: Address, target_treasury: Address, proposal_id: u64) -> Result<(), GovernanceError> {
        admin.require_auth();
        // Assume admin is validated elsewhere or we could add DataKey::Admin locally
        // For dao-fund, governance calls might be gated by proper admin authentication
        
        let mut proposal = Self::get_proposal(&env, proposal_id)?;

        if proposal.stake_returned {
            return Err(GovernanceError::StakeAlreadyReturned);
        }

        proposal.stake_returned = true;
        env.storage().instance().set(&GovernanceDataKey::Proposal(proposal_id), &proposal);

        if proposal.stake_amount > 0 {
            let stake_token = Self::get_stake_token(&env)?;
            let token_client = token::Client::new(&env, &stake_token);
            token_client.transfer(
                &env.current_contract_address(),
                &target_treasury,
                &proposal.stake_amount
            );
        }

        Ok(())
    }

    // Helper functions
    fn get_proposal_ids(env: &Env) -> Result<Vec<u64>, GovernanceError> {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::ProposalIds)
            .ok_or(GovernanceError::NotInitialized)
    }

    fn get_proposal(env: &Env, proposal_id: u64) -> Result<Proposal, GovernanceError> {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::Proposal(proposal_id))
            .ok_or(GovernanceError::ProposalNotFound)
    }

    fn get_governance_token(env: &Env) -> Result<Address, GovernanceError> {
        env.storage()
            .instance()
            .get(&GovernanceDataKey::GovernanceToken)
            .ok_or(GovernanceError::NotInitialized)
    }

    fn get_voting_threshold(env: &Env) -> Result<i128, GovernanceError> {
        env.storage().instance().get(&GovernanceDataKey::VotingThreshold).ok_or(GovernanceError::NotInitialized)
    }

    fn get_quorum_threshold(env: &Env) -> Result<i128, GovernanceError> {
        env.storage().instance().get(&GovernanceDataKey::QuorumThreshold).ok_or(GovernanceError::NotInitialized)
    }

    fn get_stake_token(env: &Env) -> Result<Address, GovernanceError> {
        env.storage().instance().get(&GovernanceDataKey::StakeToken).ok_or(GovernanceError::NotInitialized)
    }

    fn get_proposal_stake_amount(env: &Env) -> Result<i128, GovernanceError> {
        env.storage().instance().get(&GovernanceDataKey::ProposalStakeAmount).ok_or(GovernanceError::NotInitialized)
    }
}
