use bolt_lang::*;
use game_config::{GameConfig, PendingEffect, MAX_PLAYERS};
use player_registry::PlayerRegistry;

declare_id!("DP8R8Fkj2py7VMkRTH1JgWxq7nwydncKrwG8RJacKSJh");

#[error_code]
pub enum GameError {
    #[msg("Invalid config: player bounds out of range (need 2 <= min <= max <= MAX_PLAYERS)")]
    InvalidConfig,
}

/// First system the lobby calls after creating the Bolt entity for a new
/// match. Sets `GameConfig` to the WAITING state so `spawn-player` can
/// register joiners; `start-match` flips it to RUNNING once enough seats
/// are filled.
///
/// Note: `PlayerRegistry` lives on the same entity but is NOT touched
/// here — its `Default` impl pre-fills both Vec slots to MAX_PLAYERS
/// zeros with `count = 0`, which is exactly the empty state we want.
/// Bolt initialises the component on `add_component` before this system
/// fires.
#[system]
pub mod init_game {
    pub fn execute(ctx: Context<Components>, args_p: Vec<u8>) -> Result<Components> {
        let args: Args = parse_args(&args_p);
        let min_p = args.min_players.unwrap_or(2);
        let max_p = args.max_players.unwrap_or(4);

        require!(
            min_p >= 2 && (max_p as usize) <= MAX_PLAYERS && min_p <= max_p,
            GameError::InvalidConfig
        );

        let g = &mut ctx.accounts.game_config;

        // ── Identity + match configuration ─────────────────────────────
        g.match_id           = args.match_id;
        g.min_players        = min_p;
        g.max_players        = max_p;
        g.turn_duration_secs = args.turn_duration_secs.unwrap_or(15);
        g.protect_lock_turns = args.protect_lock_turns.unwrap_or(1);
        g.bullets_cap        = args.bullets_cap.unwrap_or(6);
        g.mirrors_cap        = args.mirrors_cap.unwrap_or(1);
        g.loot_mode          = 0;
        g.win_mode           = 0;

        // ── Phase machine — lobby open, no turn running yet ────────────
        // `status = 0` = Waiting (joins open). `start-match` will flip
        // this to 1 (Running) and set `turn_deadline` once enough players
        // have spawned in.
        g.status          = 0;
        g.phase           = 0;
        g.turn            = 0;
        g.active_players  = 0;
        g.alive_count     = 0;
        g.submitted_count = 0;
        g.turn_deadline   = 0;

        // ── Outcome slots — empty until end-match writes them ─────────
        g.winners      = Vec::new();
        g.winner_count = 0;

        // ── Resolve-phase scratch — pre-sized so positional writes
        //    from resolve-turn don't need to push/pop. ─────────────────
        g.pending_effects = vec![PendingEffect::default(); MAX_PLAYERS];
        g.apply_count     = 0;

        // ── Cross-component identity: PlayerRegistry mirrors the
        //    same `match_id` so resolve-turn / apply-turn can refuse
        //    a Registry that points at a different match. The
        //    `players` / `player_states` Vecs stay at their `Default`
        //    state (MAX_PLAYERS zero entries, count=0) — spawn-player
        //    fills them as joiners come in. ──────────────────────────
        ctx.accounts.player_registry.match_id = args.match_id;

        Ok(ctx.accounts)
    }

    #[system_input]
    pub struct Components {
        pub game_config: GameConfig,
        pub player_registry: PlayerRegistry,
    }

    #[arguments]
    struct Args {
        /// Match identifier — propagated by the lobby program so the
        /// off-chain indexer can join `GameConfig` to `LobbyAccount`.
        match_id: u64,
        /// Minimum seats required to call `start-match`. Defaults to 2.
        min_players: Option<u8>,
        /// Maximum seats `spawn-player` will accept. Defaults to 4.
        /// Hard ceiling is `MAX_PLAYERS` from the component (10).
        max_players: Option<u8>,
        /// How long each turn lasts. Defaults to 15s.
        turn_duration_secs: Option<u16>,
        /// Turns of Protect lockout after a hit. Defaults to 1.
        protect_lock_turns: Option<u8>,
        /// Bullet inventory cap. Defaults to 6.
        bullets_cap: Option<u8>,
        /// Mirror inventory cap. Defaults to 1.
        mirrors_cap: Option<u8>,
    }
}
