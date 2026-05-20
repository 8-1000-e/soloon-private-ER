use bolt_lang::*;
use game_config::GameConfig;
use player_registry::PlayerRegistry;
use player_state::PlayerState;

// TODO: regenerate keypair before first deploy —
//   `solana-keygen new -o target/deploy/spawn_player-keypair.json --no-bip39-passphrase`
//   `solana address -k target/deploy/spawn_player-keypair.json`
// Then paste the resulting pubkey here AND in `Anchor.toml`.
declare_id!("2En9syZVpQYtt8qF5ofVaFcT2niBa8LJcqjVfbXHwpBV");

#[error_code]
pub enum GameError {
    #[msg("Cannot join: game is not in the Waiting phase")]
    GameNotWaiting,
    #[msg("Cannot join: max_players reached")]
    LobbyFull,
    #[msg("Cannot join: player already in this match")]
    AlreadyJoined,
    #[msg("PlayerRegistry.match_id doesn't match GameConfig.match_id — wrong registry for this match")]
    RegistryMatchIdMismatch,
    #[msg("spawn-player called without the player wallet in remaining_accounts.last()")]
    MissingPlayerAuthority,
}

/// Register a new player in the match. Run once per join — the back
/// fires it on behalf of every confirmed lobby member after the
/// `join_lobby` Anchor ix has settled on L1.
///
/// Auth model:
///   The TX signer is the BACK (it orchestrates `setupMatch` for every
///   match), NOT the player wallet. The actual player wallet is
///   supplied as `remaining_accounts.last()` — read-only, no signing
///   required. The back is trusted to source these pubkeys from the
///   on-chain `LobbyAccount.players[]` roster (already validated by
///   the lobby program). This mirrors the trade-fight / stay-calm
///   spawn-player pattern.
///
/// Components touched:
/// - `player_state`     — freshly initialised, attached to a brand-new
///                        PDA created by Bolt before this system fires
///                        (one PlayerState component per join).
/// - `game_config`      — bumps `active_players` + `alive_count`.
/// - `player_registry`  — appends the authority + the new PlayerState
///                        PDA at index `count`, then bumps `count`.
#[system]
pub mod spawn_player {
    pub fn execute(ctx: Context<Components>, _args_p: Vec<u8>) -> Result<Components> {
        // ── Gate: lobby must still be open. ────────────────────────────
        require!(
            ctx.accounts.game_config.status == 0,
            GameError::GameNotWaiting
        );

        // ── Cross-match anti-collision: Bolt validates each component
        //    individually but NOT that they live on the same Bolt
        //    entity. Without this an attacker could pass game_config of
        //    match A and player_registry of match B, bumping
        //    active_players on A while writing to Registry B. The
        //    shared `match_id` (init-game writes both at the same
        //    value) lets us catch that. ───────────────────────────────
        require!(
            ctx.accounts.player_registry.match_id == ctx.accounts.game_config.match_id,
            GameError::RegistryMatchIdMismatch
        );

        // ── Player authority comes from `remaining_accounts.last()` —
        //    the back appends it after the per-system component slots.
        //    Bolt's #[system] macro consumes the prepended component
        //    accounts; whatever's left is the system's own
        //    `remaining_accounts`, which is where we look. ─────────────
        let player_authority_info = ctx
            .remaining_accounts
            .last()
            .ok_or(GameError::MissingPlayerAuthority)?;
        let authority_bytes = player_authority_info.key.to_bytes();

        // ── Capacity check against the per-match max_players, not the
        //    hard MAX_PLAYERS cap (game-config bounds-checked the user-
        //    supplied max at init-game time, so we only need this one). ─
        let idx = ctx.accounts.player_registry.count as usize;
        let max = ctx.accounts.game_config.max_players as usize;
        require!(idx < max, GameError::LobbyFull);

        // ── Dedup: same authority can't claim two seats. Linear scan
        //    over `count` slots — Vec is pre-sized to MAX_PLAYERS but
        //    only the prefix [0..count) carries real entries. ──────────
        for i in 0..idx {
            if ctx.accounts.player_registry.players[i] == authority_bytes {
                return err!(GameError::AlreadyJoined);
            }
        }

        // ── Initialise the fresh PlayerState. ──────────────────────────
        let p = &mut ctx.accounts.player_state;
        p.authority     = *player_authority_info.key;
        p.seat          = idx as u8;
        p.alive         = true;
        p.bullets       = 1;
        p.mirrors       = 1;
        p.hits_received = 0;
        p.protect_lock  = 0;
        // Per-turn submission fields — cleared until the first
        // `submit-action` call for turn 0.
        p.submitted     = false;
        p.action_type   = 0;
        p.target        = 255;

        // ── Append to the registry. ────────────────────────────────────
        let player_state_pda = ctx.accounts.player_state.key().to_bytes();
        ctx.accounts.player_registry.players[idx]       = authority_bytes;
        ctx.accounts.player_registry.player_states[idx] = player_state_pda;
        ctx.accounts.player_registry.count             += 1;

        // ── Bump the config counters. `alive_count` mirrors
        //    `active_players` until the first death. ─────────────────────
        ctx.accounts.game_config.active_players += 1;
        ctx.accounts.game_config.alive_count    += 1;

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub player_state: PlayerState,
        pub game_config: GameConfig,
        pub player_registry: PlayerRegistry,
    }
}
