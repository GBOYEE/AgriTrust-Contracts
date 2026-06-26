#![no_std]
use soroban_sdk::{contract, contractimpl, Env};

#[contract]
pub struct Treasury;

#[contractimpl]
impl Treasury {
    /// Terminal hop: value release.
    pub fn initialize(_env: Env) {}
}
