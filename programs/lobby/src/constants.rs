use anchor_lang::prelude::*;

// ── PDA seeds ─────────────────────────────────────────────────────────
pub const LOBBY_SEED: &[u8] = b"lobby";
pub const VAULT_SEED: &[u8] = b"vault";

// ── Caps (must mirror `game-config::MAX_PLAYERS`) ────────────────────
/// Hard ceiling on simultaneous players per match. Mirrors the
/// MAX_PLAYERS in `programs-ecs/components/game-config/src/lib.rs` so
/// the on-chain lobby never accepts a join that the ECS layer would
/// later reject inside `spawn-player`.
pub const MAX_PLAYERS: usize = 4;

/// Minimum seats before `start_match` can be called. Mirrors the ECS-
/// side check in `start-match` system.
pub const MIN_PLAYERS: usize = 2;

// ── Economics ─────────────────────────────────────────────────────────
/// Platform rake (5%) — taken off the pot in `distribute_prize` before
/// the remaining 95% is split among winners.
pub const RAKE_BPS: u64 = 500;

/// Tiny fee charged on `leave_lobby` to cover the back's tx-relay cost
/// + discourage join/leave spam. Stays in the vault (= adds to the
/// pot) rather than burning.
pub const LEAVE_FEE_LAMPORTS: u64 = 200_000; // 0.0002 SOL

/// House wallet. Receives:
///   - the 5% rake on every distributed pot
///   - 100% of the pot on a 100-turn timeout (no winner)
/// Keep this synced with the off-chain treasury config.
pub const RAKE_AUTHORITY: Pubkey =
    anchor_lang::pubkey!("J3WUUZagmoqLKJueWB2CQeUiWcbe4LmeAD9qujB6xz1B");

// ── Lobby status ──────────────────────────────────────────────────────
/// Open: players can join/leave. `start_match` is the only transition out.
pub const STATUS_OPEN: u8 = 0;
/// Started: match is running in the ECS layer. Joins/leaves rejected;
/// distribute_prize lands once ECS flips GameConfig to Finished.
pub const STATUS_STARTED: u8 = 1;
/// Finished: pot was distributed. Only `close_lobby` left to do (= rent
/// recovery to the authority).
pub const STATUS_FINISHED: u8 = 2;
/// Cancelled: authority killed the lobby before settlement. Players
/// reclaim their entry fee via `claim_refund`.
pub const STATUS_CANCELLED: u8 = 3;

// ── ECS cross-program identity ────────────────────────────────────────
/// Program ID of the `game-config` component crate. `distribute_prize`
/// verifies the GameConfig account passed in is owned by this program
/// (= the canonical Soloon match config) before trusting `winners`.
///
/// Must match `declare_id!` in
/// `programs-ecs/components/game-config/src/lib.rs`.
pub const GAME_CONFIG_PROGRAM_ID: Pubkey =
    anchor_lang::pubkey!("EcDbJA6zALpmc6te6r9CTpYHJgLYU9YhPJcTsMcPannB");
