use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod state;

pub use instructions::*;

declare_id!("ME4mKRb4XBeFkzxsyiHEjd82Lx5cX4BUudXpno2C6DL");

/// Soloon match lobby — escrow + roster + payout.
///
/// Flow:
///   1. `create_lobby` — authority opens a fresh lobby + vault PDA pair
///      keyed by `lobby_id`. Status starts at OPEN.
///   2. `join_lobby` — players pay the entry fee into the vault and
///      claim a seat. Linear-scan dedup. Caps at `lobby.max_players`.
///   3. `leave_lobby` — pre-start exit. Refunds `entry_fee - LEAVE_FEE`;
///      the small fee stays in the pot.
///   4. `start_match` — authority locks the lobby, records the Bolt
///      entity that hosts the match's ECS components, flips status to
///      STARTED. No more joins/leaves.
///   5. `distribute_prize` — authority calls AFTER the ECS layer has
///      resolved the match (= GameConfig.status == Finished). Reads
///      `GameConfig.winners` directly on-chain (verifies owner + entity
///      tie), takes the 5% rake, splits the rest 95% equally among the
///      listed winners. PER-aware: nothing about who-won is trusted to
///      the caller — the canonical source is GameConfig.
///   6. `cancel_lobby` — authority bails out (open OR started). Status
///      → CANCELLED.
///   7. `claim_refund` — per-player refund path for cancelled lobbies.
///   8. `close_lobby` — authority reclaims rent. Vault must be empty.
#[program]
pub mod lobby {
    use super::*;

    pub fn create_lobby(
        ctx: Context<CreateLobby>,
        lobby_id: u64,
        entry_fee: u64,
        max_players: u8,
    ) -> Result<()> {
        instructions::create_lobby::create_lobby(ctx, lobby_id, entry_fee, max_players)
    }

    pub fn join_lobby(ctx: Context<JoinLobby>, lobby_id: u64) -> Result<()> {
        instructions::join_lobby::join_lobby(ctx, lobby_id)
    }

    pub fn leave_lobby(ctx: Context<LeaveLobby>, lobby_id: u64) -> Result<()> {
        instructions::leave_lobby::leave_lobby(ctx, lobby_id)
    }

    pub fn start_match(
        ctx: Context<StartMatch>,
        lobby_id: u64,
        match_entity: Pubkey,
    ) -> Result<()> {
        instructions::start_match::start_match(ctx, lobby_id, match_entity)
    }

    pub fn distribute_prize(
        ctx: Context<DistributePrize>,
        lobby_id: u64,
    ) -> Result<()> {
        instructions::distribute_prize::distribute_prize(ctx, lobby_id)
    }

    pub fn cancel_lobby(ctx: Context<CancelLobby>, lobby_id: u64) -> Result<()> {
        instructions::cancel_lobby::cancel_lobby(ctx, lobby_id)
    }

    pub fn claim_refund(ctx: Context<ClaimRefund>, lobby_id: u64) -> Result<()> {
        instructions::claim_refund::claim_refund(ctx, lobby_id)
    }

    pub fn close_lobby(ctx: Context<CloseLobby>, lobby_id: u64) -> Result<()> {
        instructions::close_lobby::close_lobby(ctx, lobby_id)
    }
}
