#![no_std]

use soroban_sdk::{
    contracterror, contracttype, symbol_short, Address, Env, 
    token, Vec, Map, IntoVal, TryIntoVal, TryFromVal, Symbol,
};
use crate::storage_keys::StorageKey;

pub struct YieldTreasuryContract;

// Yield-bearing status flags
pub const YIELD_STATUS_INACTIVE: u32 = 0b00000000;
pub const YIELD_STATUS_INVESTING: u32 = 0b00000001;
pub const YIELD_STATUS_INVESTED: u32 = 0b00000010;
pub const YIELD_STATUS_DIVESTING: u32 = 0b00000100;
pub const YIELD_STATUS_EMERGENCY: u32 = 0b00001000;

// Yield strategy types
pub const YIELD_STRATEGY_STELLAR_AQUA: u32 = 1;
pub const YIELD_STRATEGY_STELLAR_USDC: u32 = 2;
pub const YIELD_STRATEGY_LIQUIDITY_POOL: u32 = 3;
pub const YIELD_STRATEGY_EXTERNAL_VAULT: u32 = 4;

/// Interface for external Soroban vaults
pub trait VaultInterface {
    fn deposit(env: Env, from: Address, amount: i128) -> i128;
    fn withdraw(env: Env, to: Address, amount: i128) -> i128;
    fn get_total_value(env: Env) -> i128;
    fn get_share_price(env: Env) -> i128;
}

#[derive(Clone)]
#[contracttype]
pub struct YieldPosition {
    pub strategy: u32,
    pub invested_amount: i128,
    pub current_value: i128,
    pub accrued_yield: i128,
    pub invested_at: u64,
    pub last_yield_update: u64,
    pub apy: i128, // Annual Percentage Yield (basis points, e.g., 500 = 5%)
}

#[derive(Clone)]
#[contracttype]
pub struct TreasuryConfig {
    pub admin: Address,
    pub min_reserve_ratio: i128, // Minimum percentage to keep as reserve (basis points)
    pub max_investment_ratio: i128, // Maximum percentage to invest (basis points)
    pub auto_invest: bool,
    pub yield_strategy: u32,
    pub emergency_withdrawal_enabled: bool,
    pub max_slippage: u32, // Slippage tolerance in basis points
    pub external_vault: Option<Address>,
}

#[derive(Clone)]
#[contracttype]
pub struct YieldMetrics {
    pub total_invested: i128,
    pub total_yield_earned: i128,
    pub current_apy: i128,
    pub last_yield_calculation: u64,
    pub investment_count: u32,
}

// Legacy DataKey alias preserved for backward compatibility.
// All runtime storage uses `StorageKey` directly.
type DataKey = StorageKey;

#[contracterror]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum YieldError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NotAuthorized = 3,
    InsufficientReserve = 4,
    InsufficientInvestment = 5,
    InvalidAmount = 6,
    InvalidStrategy = 7,
    InvestmentActive = 8,
    InvestmentInactive = 9,
    MathOverflow = 10,
    YieldCalculationFailed = 11,
    EmergencyMode = 12,
    TokenError = 13,
    InvalidState = 14,
}

// Helper functions
fn read_admin(env: &Env) -> Result<Address, YieldError> {
    env.storage()
        .instance()
        .get(&StorageKey::Admin)
        .ok_or(YieldError::NotInitialized)
}

fn require_admin_auth(env: &Env) -> Result<(), YieldError> {
    let admin = read_admin(env)?;
    admin.require_auth();
    Ok(())
}

fn read_config(env: &Env) -> Result<TreasuryConfig, YieldError> {
    env.storage()
        .instance()
        .get(&StorageKey::Config)
        .ok_or(YieldError::NotInitialized)
}

fn write_config(env: &Env, config: &TreasuryConfig) {
    env.storage().instance().set(&StorageKey::Config, config);
}

fn read_yield_position(env: &Env) -> Result<YieldPosition, YieldError> {
    env.storage()
        .instance()
        .get(&StorageKey::YieldPosition)
        .ok_or(YieldError::InvestmentInactive)
}

fn write_yield_position(env: &Env, position: &YieldPosition) {
    env.storage().instance().set(&StorageKey::YieldPosition, position);
}

fn read_metrics(env: &Env) -> Result<YieldMetrics, YieldError> {
    env.storage()
        .instance()
        .get(&StorageKey::Metrics)
        .ok_or(YieldError::NotInitialized)
}

fn write_metrics(env: &Env, metrics: &YieldMetrics) {
    env.storage().instance().set(&StorageKey::Metrics, metrics);
}

fn read_reserve_balance(env: &Env) -> Result<i128, YieldError> {
    env.storage()
        .instance()
        .get(&StorageKey::ReserveBalance)
        .ok_or(YieldError::NotInitialized)
}

fn write_reserve_balance(env: &Env, balance: i128) {
    env.storage().instance().set(&StorageKey::ReserveBalance, &balance);
}

fn read_yield_token(env: &Env) -> Result<token::Client, YieldError> {
    let token_address = env
        .storage()
        .instance()
        .get(&StorageKey::YieldToken)
        .ok_or(YieldError::NotInitialized)?;
    Ok(token::Client::new(env, &token_address))
}

fn calculate_yield_amount(position: &YieldPosition, now: u64) -> Result<i128, YieldError> {
    if now <= position.last_yield_update {
        return Ok(0);
    }

    let time_elapsed = now - position.last_yield_update;
    let seconds_in_year = 365u64 * 24u64 * 60u64 * 60u64;
    
    // Calculate yield for the elapsed time
    // yield = invested_amount * apy * time_elapsed / (10000 * seconds_in_year)
    let time_ratio = i128::from(time_elapsed);
    let year_ratio = i128::from(seconds_in_year);
    
    let yield_amount = position
        .invested_amount
        .checked_mul(position.apy)
        .ok_or(YieldError::MathOverflow)?
        .checked_mul(time_ratio)
        .ok_or(YieldError::MathOverflow)?
        .checked_div(10000) // Convert basis points to decimal
        .ok_or(YieldError::MathOverflow)?
        .checked_div(year_ratio)
        .ok_or(YieldError::MathOverflow)?;

    Ok(yield_amount)
}

fn update_yield_position(env: &Env, position: &mut YieldPosition) -> Result<(), YieldError> {
    let now = env.ledger().timestamp();
    let new_yield = calculate_yield_amount(position, now)?;
    
    position.accrued_yield = position
        .accrued_yield
        .checked_add(new_yield)
        .ok_or(YieldError::MathOverflow)?;
    
    position.current_value = position
        .invested_amount
        .checked_add(position.accrued_yield)
        .ok_or(YieldError::MathOverflow)?;
    
    position.last_yield_update = now;
    
    Ok(())
}

fn preview_yield_position_value(position: &YieldPosition, now: u64) -> Result<i128, YieldError> {
    if now <= position.last_yield_update {
        return Ok(position.current_value);
    }

    let yield_since_update = calculate_yield_amount(position, now)?;
    position.current_value
        .checked_add(yield_since_update)
        .ok_or(YieldError::MathOverflow)
}

fn ensure_reserve_ratio(
    env: &Env, 
    total_balance: i128, 
    investment_amount: i128
) -> Result<(), YieldError> {
    let config = read_config(env)?;
    let reserve_needed = total_balance
        .checked_mul(config.min_reserve_ratio)
        .ok_or(YieldError::MathOverflow)?
        .checked_div(10000)
        .ok_or(YieldError::MathOverflow)?;
    
    let remaining_reserve = total_balance
        .checked_sub(investment_amount)
        .ok_or(YieldError::InsufficientReserve)?;
    
    if remaining_reserve < reserve_needed {
        return Err(YieldError::InsufficientReserve);
    }
    
    Ok(())
}

impl YieldTreasuryContract {
    pub fn preview_pool_health(env: Env, total_liabilities: i128) -> Result<i128, YieldError> {
        let reserve = read_reserve_balance(&env)?;
        let mut total_assets = reserve;

        if let Ok(position) = read_yield_position(&env) {
            let now = env.ledger().timestamp();
            let current_value = preview_yield_position_value(&position, now)?;
            total_assets = total_assets.checked_add(current_value).ok_or(YieldError::MathOverflow)?;
        }

        if total_liabilities <= 0 {
            return Ok(10000);
        }

        let risk_adjusted_assets = total_assets.checked_mul(9000).ok_or(YieldError::MathOverflow)? / 10000;
        let health_factor = risk_adjusted_assets
            .checked_mul(10000)
            .ok_or(YieldError::MathOverflow)?
            .checked_div(total_liabilities)
            .ok_or(YieldError::MathOverflow)?;

        Ok(health_factor)
    }

    /// Initialize the yield treasury contract
    pub fn initialize(
        env: Env,
        admin: Address,
        yield_token_address: Address,
        initial_config: TreasuryConfig,
    ) -> Result<(), YieldError> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(YieldError::AlreadyInitialized);
        }
        
        admin.require_auth();
        
        // Set admin
        env.storage().instance().set(&StorageKey::Admin, &admin);
        
        // Set yield token
        env.storage().instance().set(&StorageKey::YieldToken, &yield_token_address);
        
        // Initialize config
        let config = TreasuryConfig {
            admin: admin.clone(),
            min_reserve_ratio: initial_config.min_reserve_ratio,
            max_investment_ratio: initial_config.max_investment_ratio,
            auto_invest: initial_config.auto_invest,
            yield_strategy: initial_config.yield_strategy,
            emergency_withdrawal_enabled: initial_config.emergency_withdrawal_enabled,
            max_slippage: initial_config.max_slippage,
            external_vault: initial_config.external_vault,
        };
        write_config(&env, &config);
        
        // Initialize metrics
        let metrics = YieldMetrics {
            total_invested: 0,
            total_yield_earned: 0,
            current_apy: 0,
            last_yield_calculation: env.ledger().timestamp(),
            investment_count: 0,
        };
        write_metrics(&env, &metrics);
        
        // Initialize reserve balance
        write_reserve_balance(&env, 0);
        
        env.events().publish(
            (symbol_short!("yld_init"), admin.clone(), yield_token_address.clone()),
            (admin, yield_token_address),
        );
        
        Ok(())
    }
    
    /// Invest idle funds into yield-bearing strategy
    pub fn invest_idle_funds(
        env: Env,
        amount: i128,
        strategy: Option<u32>,
    ) -> Result<(), YieldError> {
        require_admin_auth(&env)?;
        
        if amount <= 0 {
            return Err(YieldError::InvalidAmount);
        }
        
        let config = read_config(&env)?;
        let yield_token = read_yield_token(&env)?;
        
        // Check if there's an active investment
        if let Ok(_position) = read_yield_position(&env) {
            return Err(YieldError::InvestmentActive);
        }
        
        // Get current contract balance
        let contract_balance = yield_token.balance(&env.current_contract_address());
        
        // Ensure minimum reserve is maintained
        ensure_reserve_ratio(&env, contract_balance, amount)?;
        
        // Determine strategy
        let investment_strategy = strategy.unwrap_or(config.yield_strategy);
        match investment_strategy {
            YIELD_STRATEGY_STELLAR_AQUA
            | YIELD_STRATEGY_STELLAR_USDC
            | YIELD_STRATEGY_LIQUIDITY_POOL
            | YIELD_STRATEGY_EXTERNAL_VAULT => {},
            _ => return Err(YieldError::InvalidStrategy),
        }

        if investment_strategy == YIELD_STRATEGY_EXTERNAL_VAULT {
            let vault_addr = config.external_vault.as_ref().ok_or(YieldError::InvalidStrategy)?;
            
            // Approve vault to spend tokens
            yield_token.approve(&env.current_contract_address(), vault_addr, &amount, &(env.ledger().sequence().saturating_add(500)));
            
            // Call external vault deposit
            let shares_minted: i128 = env.invoke_contract(
                vault_addr,
                &Symbol::new(&env, "deposit"),
                Vec::from_array(&env, [env.current_contract_address().into_val(&env), amount.into_val(&env)]),
            );
            
            if shares_minted <= 0 {
                return Err(YieldError::TokenError);
            }
        } else {
            // Transfer tokens from contract to internal yield tracking (for mock strategies)
            yield_token.transfer(&env.current_contract_address(), &env.current_contract_address(), &amount);
        }
        
        // Create yield position
        let now = env.ledger().timestamp();
        let apy = Self::get_strategy_apy(&env, investment_strategy)?;
        
        let position = YieldPosition {
            strategy: investment_strategy,
            invested_amount: amount,
            current_value: amount,
            accrued_yield: 0,
            invested_at: now,
            last_yield_update: now,
            apy,
        };
        
        write_yield_position(&env, &position);
        
        // Update reserve balance
        let new_reserve = contract_balance
            .checked_sub(amount)
            .ok_or(YieldError::InsufficientReserve)?;
        write_reserve_balance(&env, new_reserve);
        
        // Update metrics
        let mut metrics = read_metrics(&env)?;
        metrics.total_invested = metrics
            .total_invested
            .checked_add(amount)
            .ok_or(YieldError::MathOverflow)?;
        metrics.investment_count += 1;
        metrics.current_apy = apy;
        metrics.last_yield_calculation = now;
        write_metrics(&env, &metrics);
        
        let admin = read_admin(&env)?;
        let yield_token = env.storage().instance().get::<_, Address>(&StorageKey::YieldToken).unwrap();
        env.events().publish(
            (symbol_short!("yield_inv"), admin, yield_token, investment_strategy),
            (amount, apy),
        );
        
        Ok(())
    }
    
    /// Divest funds from yield-bearing strategy
    pub fn divest_funds(
        env: Env,
        amount: Option<i128>, // None = divest all
    ) -> Result<(), YieldError> {
        require_admin_auth(&env)?;
        
        let mut position = read_yield_position(&env)?;
        let yield_token = read_yield_token(&env)?;
        
        // Update yield position with accrued yield
        update_yield_position(&env, &mut position)?;
        
        // Determine divestment amount
        let divest_amount = match amount {
            Some(amt) => {
                if amt <= 0 {
                    return Err(YieldError::InvalidAmount);
                }
                if amt > position.current_value {
                    return Err(YieldError::InsufficientInvestment);
                }
                amt
            },
            None => position.current_value, // Divest all
        };
        
        // Calculate remaining position
        let remaining_value = position
            .current_value
            .checked_sub(divest_amount)
            .ok_or(YieldError::MathOverflow)?;
        
        // Calculate proportional investment and yield
        let investment_ratio = if position.current_value > 0 {
            position.invested_amount
                .checked_mul(10000)
                .ok_or(YieldError::MathOverflow)?
                .checked_div(position.current_value)
                .ok_or(YieldError::MathOverflow)?
        } else {
            10000 // 100%
        };
        
        let investment_return = divest_amount
            .checked_mul(investment_ratio)
            .ok_or(YieldError::MathOverflow)?
            .checked_div(10000)
            .ok_or(YieldError::MathOverflow)?;
        
        let yield_return = divest_amount
            .checked_sub(investment_return)
            .ok_or(YieldError::MathOverflow)?;
        
        // Update position
        position.invested_amount = position
            .invested_amount
            .checked_sub(investment_return)
            .ok_or(YieldError::MathOverflow)?;
        
        position.accrued_yield = position
            .accrued_yield
            .checked_sub(yield_return)
            .ok_or(YieldError::MathOverflow)?;
        
        position.current_value = remaining_value;
        
        // If fully divested, remove position
        if remaining_value == 0 {
            env.storage().instance().remove(&StorageKey::YieldPosition);
        } else {
            write_yield_position(&env, &position);
        }
        
        // Update reserve balance
        let current_reserve = read_reserve_balance(&env)?;
        let new_reserve = current_reserve
            .checked_add(divest_amount)
            .ok_or(YieldError::MathOverflow)?;
        write_reserve_balance(&env, new_reserve);
        
        // Update metrics
        let mut metrics = read_metrics(&env)?;
        metrics.total_yield_earned = metrics
            .total_yield_earned
            .checked_add(yield_return)
            .ok_or(YieldError::MathOverflow)?;
        
        if remaining_value == 0 {
            metrics.current_apy = 0;
        }
        metrics.last_yield_calculation = env.ledger().timestamp();
        write_metrics(&env, &metrics);
        
        let admin = read_admin(&env)?;
        let yield_token = env.storage().instance().get::<_, Address>(&StorageKey::YieldToken).unwrap();
        env.events().publish(
            (symbol_short!("yield_div"), admin, yield_token),
            (divest_amount, investment_return, yield_return),
        );
        
        Ok(())
    }
    
    /// Get current yield position
    pub fn get_yield_position(env: Env) -> Result<YieldPosition, YieldError> {
        let mut position = read_yield_position(&env)?;
        update_yield_position(&env, &mut position)?;
        Ok(position)
    }
    
    /// Get treasury metrics
    pub fn get_yield_metrics(env: Env) -> Result<YieldMetrics, YieldError> {
        let mut metrics = read_metrics(&env)?;
        
        // Update current yield if position exists
        if let Ok(mut position) = read_yield_position(&env) {
            update_yield_position(&env, &mut position)?;
            metrics.current_apy = position.apy;
        }
        
        Ok(metrics)
    }
    
    /// Get reserve balance
    pub fn get_reserve_balance(env: Env) -> Result<i128, YieldError> {
        read_reserve_balance(&env)
    }
    
    /// Get total contract balance (reserve + invested)
    pub fn get_total_balance(env: Env) -> Result<i128, YieldError> {
        let yield_token = read_yield_token(&env)?;
        let contract_balance = yield_token.balance(&env.current_contract_address());
        Ok(contract_balance)
    }
    
    /// Update treasury configuration
    pub fn update_config(
        env: Env,
        new_config: TreasuryConfig,
    ) -> Result<(), YieldError> {
        require_admin_auth(&env)?;
        write_config(&env, &new_config);
        
        env.events().publish(
            (symbol_short!("cfg_updt"),),
            (new_config.min_reserve_ratio, new_config.max_investment_ratio),
        );
        
        Ok(())
    }
    
    /// Emergency withdrawal - bypass all checks
    pub fn emergency_withdraw(
        env: Env,
        amount: i128,
        recipient: Address,
    ) -> Result<(), YieldError> {
        require_admin_auth(&env)?;
        
        let config = read_config(&env)?;
        if !config.emergency_withdrawal_enabled {
            return Err(YieldError::EmergencyMode);
        }
        
        if amount <= 0 {
            return Err(YieldError::InvalidAmount);
        }
        
        let yield_token = read_yield_token(&env)?;
        let contract_balance = yield_token.balance(&env.current_contract_address());
        
        if amount > contract_balance {
            return Err(YieldError::InsufficientInvestment);
        }
        
        // Transfer to recipient
        yield_token.transfer(&env.current_contract_address(), &recipient, &amount);
        
        env.events().publish(
            (symbol_short!("em_wdraw"),),
            (amount, recipient),
        );
        
        Ok(())
    }
    
    /// Get APY for a specific strategy
    fn get_strategy_apy(env: &Env, strategy: u32) -> Result<i128, YieldError> {
        // In a real implementation, these would be fetched from oracles or protocols
        // For now, using mock APY values
        match strategy {
            YIELD_STRATEGY_STELLAR_AQUA => Ok(800), // 8% APY
            YIELD_STRATEGY_STELLAR_USDC => Ok(500), // 5% APY
            YIELD_STRATEGY_LIQUIDITY_POOL => Ok(1200), // 12% APY
            _ => Err(YieldError::InvalidStrategy),
        }
    }
    
    /// Auto-invest idle funds (can be called by anyone)
    pub fn auto_invest(env: Env) -> Result<(), YieldError> {
        let config = read_config(&env)?;
        
        if !config.auto_invest {
            return Err(YieldError::InvalidState);
        }
        
        // Check if there's already an active investment
        if read_yield_position(&env).is_ok() {
            return Err(YieldError::InvestmentActive);
        }
        
        let yield_token = read_yield_token(&env)?;
        let contract_balance = yield_token.balance(&env.current_contract_address());
        let reserve_balance = read_reserve_balance(&env)?;
        
        // Calculate available idle funds
        let idle_funds = contract_balance
            .checked_sub(reserve_balance)
            .ok_or(YieldError::InsufficientReserve)?;
        
        // Calculate maximum investment based on ratio
        let max_investment = contract_balance
            .checked_mul(config.max_investment_ratio)
            .ok_or(YieldError::MathOverflow)?
            .checked_div(10000)
            .ok_or(YieldError::MathOverflow)?;
        
        let investment_amount = if idle_funds > max_investment {
            max_investment
        } else {
            idle_funds
        };
        
        if investment_amount > 0 {
            Self::invest_idle_funds(env, investment_amount, Some(config.yield_strategy))?;
        }
        
        Ok(())
    }
    
    /// Check if investment is active
    pub fn is_investment_active(env: Env) -> Result<bool, YieldError> {
        Ok(read_yield_position(&env).is_ok())
    }

    /// Harvest yield from the current strategy and account for it
    pub fn harvest_yield(env: Env) -> Result<i128, YieldError> {
        let mut position = read_yield_position(&env)?;
        let config = read_config(&env)?;
        
        let mut actual_yield = 0_i128;
        
        if position.strategy == YIELD_STRATEGY_EXTERNAL_VAULT {
            let vault_addr = config.external_vault.as_ref().ok_or(YieldError::InvalidStrategy)?;
            
            // Get current value from external vault
            let total_value: i128 = env.invoke_contract(
                vault_addr,
                &Symbol::new(&env, "get_total_value"),
                Vec::new(&env),
            );
            
            if total_value > position.current_value {
                actual_yield = total_value - position.current_value;
            }
            
            position.current_value = total_value;
            position.accrued_yield = position.accrued_yield.checked_add(actual_yield).ok_or(YieldError::MathOverflow)?;
            position.last_yield_update = env.ledger().timestamp();
        } else {
            // Internal mock strategies
            update_yield_position(&env, &mut position)?;
            actual_yield = calculate_yield_amount(&position, env.ledger().timestamp())?;
        }
        
        write_yield_position(&env, &position);
        
        // Update metrics
        let mut metrics = read_metrics(&env)?;
        metrics.total_yield_earned = metrics.total_yield_earned.checked_add(actual_yield).ok_or(YieldError::MathOverflow)?;
        metrics.last_yield_calculation = env.ledger().timestamp();
        write_metrics(&env, &metrics);
        
        let admin = read_admin(&env)?;
        let yield_token = env.storage().instance().get::<_, Address>(&StorageKey::YieldToken).unwrap();
        env.events().publish(
            (symbol_short!("harvest"), admin, yield_token),
            (actual_yield, position.current_value),
        );
        
        Ok(actual_yield)
    }

    /// Calculate pool health factor (0.0 to 1.0 represented as 0 to 10000 bps)
    pub fn calculate_pool_health(env: Env, total_liabilities: i128) -> Result<i128, YieldError> {
        let reserve = read_reserve_balance(&env)?;
        let mut total_assets = reserve;
        
        if let Ok(mut position) = read_yield_position(&env) {
            update_yield_position(&env, &mut position)?;
            total_assets = total_assets.checked_add(position.current_value).ok_or(YieldError::MathOverflow)?;
        }
        
        if total_liabilities <= 0 {
            return Ok(10000); // 1.0 if no liabilities
        }
        
        // Mock volatility: 10% reduction in asset value for risk calculation
        let risk_adjusted_assets = total_assets.checked_mul(9000).ok_or(YieldError::MathOverflow)? / 10000;
        
        let health_factor = risk_adjusted_assets
            .checked_mul(10000)
            .ok_or(YieldError::MathOverflow)?
            .checked_div(total_liabilities)
            .ok_or(YieldError::MathOverflow)?;
            
        if health_factor < 10000 {
            let admin = read_admin(&env)?;
            env.events().publish(
                (symbol_short!("risk_warn"), admin),
                (health_factor, total_assets, total_liabilities),
            );
        }
        
        Ok(health_factor)
    }
}
