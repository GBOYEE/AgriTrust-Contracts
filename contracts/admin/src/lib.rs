#![no_std]
use soroban_sdk::{contract, contractimpl, Env};

#[contract]
pub struct Admin;

#[contractimpl]
impl Admin {
    /// Policy resolution hop.
    pub fn initialize(_env: Env) {}
}
