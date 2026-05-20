use anchor_lang::prelude::*;
use crate::constants::MAX_PLAYERS;

/// Lobby metadata + roster. One per `lobby_id`.
/// Seeds: `["lobby", lobby_id.to_le_bytes()]`. Rent-exempt + owned by
/// this program.
///
/// The actual entry-fee escrow lives on the separate `Vault` PDA (see
/// below) — keeping rent + pot in different accounts means the lobby's
/// rent-exempt minimum stays constant as players join/leave.
#[account]
pub struct LobbyAccount {
    pub lobby_id: u64,
    /// Back-side signer that controls start/cancel/distribute/close.
    /// TOFU — captured from the `create_lobby` signer.
    pub authority: Pubkey,
    /// Lamports each `join_lobby` transfers to the vault.
    pub entry_fee: u64,
    /// Max seats this match accepts. 2..=MAX_PLAYERS.
    pub max_players: u8,
    /// Number of seats currently filled.
    pub player_count: u8,
    /// Registered player wallets. Slots beyond `player_count` are zero.
    pub players: [Pubkey; MAX_PLAYERS],
    /// 0 = Open, 1 = Started, 2 = Finished, 3 = Cancelled.
    pub status: u8,
    /// Bolt entity PDA for the match's ECS components. Set by
    /// `start_match` and used by `distribute_prize` to verify the
    /// GameConfig account passed in is bound to THIS match.
    pub match_entity: Pubkey,
    /// Unix-second timestamp from `create_lobby`.
    pub created_at: i64,
    /// Unix-second timestamp from `start_match` (0 before).
    pub started_at: i64,
    pub bump: u8,
}

impl LobbyAccount {
    /// 8 disc + 8 id + 32 auth + 8 fee + 1 max + 1 count
    /// + 32 * MAX_PLAYERS players + 1 status + 32 entity
    /// + 8 created + 8 started + 1 bump
    pub const LEN: usize =
        8 + 8 + 32 + 8 + 1 + 1 + (32 * MAX_PLAYERS) + 1 + 32 + 8 + 8 + 1;
}

/// Pure-escrow account that holds the pot lamports for a lobby.
/// Seeds: `["vault", lobby.key()]`. Separating the pot from the
/// LobbyAccount lets us sub_lamports() / add_lamports() without
/// worrying about the lobby's own rent-exempt minimum, and gives the
/// audit trail a clean "where the money is" pubkey.
#[account]
pub struct Vault {
    /// Back-reference to the lobby this vault belongs to. Anchor's
    /// PDA seed check already enforces this, but the field is useful
    /// for off-chain indexers.
    pub lobby: Pubkey,
    /// Running pot total in lamports — bumped on `join_lobby`,
    /// decremented on `leave_lobby` / `claim_refund` / `distribute_prize`.
    pub total_pot: u64,
    pub bump: u8,
}

impl Vault {
    /// 8 disc + 32 lobby + 8 pot + 1 bump = 49.
    pub const LEN: usize = 8 + 32 + 8 + 1;
}
