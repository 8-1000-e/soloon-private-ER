use anchor_lang::prelude::*;

#[error_code]
pub enum LobbyError {
    #[msg("Lobby is not open")]                         LobbyNotOpen,
    #[msg("Lobby is full (max_players reached)")]       LobbyFull,
    #[msg("Player already joined this lobby")]          AlreadyJoined,
    #[msg("Player not in this lobby")]                  PlayerNotFound,
    #[msg("Not enough players to start (need MIN_PLAYERS)")]
                                                         NotEnoughPlayers,
    #[msg("Match already started")]                     MatchAlreadyStarted,
    #[msg("Match has not started yet")]                 MatchNotStarted,
    #[msg("max_players out of range (2..=MAX_PLAYERS)")]
                                                         InvalidMaxPlayers,
    #[msg("entry_fee too low — must cover at least 2× LEAVE_FEE_LAMPORTS so refunds stay non-zero")]
                                                         EntryFeeTooLow,
    #[msg("Unauthorized — caller is not the lobby authority")]
                                                         Unauthorized,
    #[msg("Rake authority account doesn't match the hardcoded house pubkey")]
                                                         InvalidRakeAuthority,
    #[msg("Cannot close lobby: vault is not empty (distribute prize or refund first)")]
                                                         VaultNotEmpty,
    #[msg("Lobby is not in a closable state (must be Finished or Cancelled)")]
                                                         NotClosable,

    // ── ECS cross-check errors (PER-aware distribute_prize) ──────────
    #[msg("GameConfig account is not owned by the game-config program")]
                                                         InvalidGameConfigOwner,
    #[msg("GameConfig.match_id doesn't match lobby.lobby_id (wrong match config)")]
                                                         GameConfigEntityMismatch,
    #[msg("Match has not finished yet in the ECS layer (GameConfig.status != Finished)")]
                                                         MatchNotFinished,
    #[msg("Winner account passed via remaining_accounts doesn't match GameConfig.winners[i]")]
                                                         WrongWinner,
    #[msg("Winner count between GameConfig.winner_count and remaining_accounts mismatch")]
                                                         WinnerCountMismatch,
}
