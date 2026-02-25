use anchor_lang::prelude::*;
use anchor_lang::system_program;

declare_id!("4Uu75QspEnoCzdDp8QWQkMCfbL5aY6xkk14mccziPtdB");

// Status constants
const STATUS_OPEN: u8 = 0;
const STATUS_STARTED: u8 = 1;
const STATUS_FINISHED: u8 = 2;
const STATUS_CANCELLED: u8 = 3;

#[error_code]
pub enum LobbyError {
    #[msg("Lobby is not open")]
    LobbyNotOpen,
    #[msg("Lobby is full")]
    LobbyFull,
    #[msg("Player already joined")]
    AlreadyJoined,
    #[msg("Player not found in lobby")]
    PlayerNotFound,
    #[msg("Not enough players to start (min 2)")]
    NotEnoughPlayers,
    #[msg("Match already started")]
    MatchAlreadyStarted,
    #[msg("Match not started")]
    MatchNotStarted,
    #[msg("Invalid winner index")]
    InvalidWinnerIndex,
    #[msg("Winner account does not match")]
    WrongWinner,
    #[msg("max_players must be between 2 and 4")]
    InvalidMaxPlayers,
    #[msg("Unauthorized")]
    Unauthorized,
}

// Account size: 8 discriminator + 32 authority + 8 lobby_id + 8 entry_fee
//               + 1 max_players + 4*32 players + 1 player_count
//               + 1 status + 8 pot + 32 match_entity + 1 bump
// = 8 + 32 + 8 + 8 + 1 + 128 + 1 + 1 + 8 + 32 + 1 = 228
pub const LOBBY_SIZE: usize = 228;

#[account]
pub struct LobbyAccount {
    pub authority: Pubkey,       // 32 – must sign create/start/distribute/cancel
    pub lobby_id: u64,           // 8
    pub entry_fee: u64,          // 8  – lamports per player (e.g. 10_000_000 = 0.01 SOL)
    pub max_players: u8,         // 1  – 2..=4
    pub players: [Pubkey; 4],    // 128 – registered player wallets
    pub player_count: u8,        // 1
    pub status: u8,              // 1  – 0=Open,1=Started,2=Finished,3=Cancelled
    pub pot: u64,                // 8  – total lamports collected (entry_fee * player_count)
    pub match_entity: Pubkey,    // 32 – BOLT entity PDA set on start_match
    pub bump: u8,                // 1
}

#[program]
pub mod lobby {
    use super::*;

    /// Authority creates a lobby with a fixed entry fee and max player count.
    pub fn create_lobby(
        ctx: Context<CreateLobby>,
        lobby_id: u64,
        entry_fee: u64,
        max_players: u8,
    ) -> Result<()> {
        require!(
            max_players >= 2 && max_players <= 4,
            LobbyError::InvalidMaxPlayers
        );

        let lobby = &mut ctx.accounts.lobby;
        lobby.authority = ctx.accounts.authority.key();
        lobby.lobby_id = lobby_id;
        lobby.entry_fee = entry_fee;
        lobby.max_players = max_players;
        lobby.players = [Pubkey::default(); 4];
        lobby.player_count = 0;
        lobby.status = STATUS_OPEN;
        lobby.pot = 0;
        lobby.match_entity = Pubkey::default();
        lobby.bump = ctx.bumps.lobby;

        msg!(
            "Lobby {} created: entry_fee={} lamports, max_players={}",
            lobby_id,
            entry_fee,
            max_players
        );
        Ok(())
    }

    /// Player joins the lobby by transferring the entry fee.
    pub fn join_lobby(ctx: Context<JoinLobby>, _lobby_id: u64) -> Result<()> {
        let player_key = ctx.accounts.player.key();

        // All checks before mutably borrowing lobby
        {
            let lobby = &ctx.accounts.lobby;
            require!(lobby.status == STATUS_OPEN, LobbyError::LobbyNotOpen);
            require!(
                lobby.player_count < lobby.max_players,
                LobbyError::LobbyFull
            );
            for i in 0..lobby.player_count as usize {
                require!(lobby.players[i] != player_key, LobbyError::AlreadyJoined);
            }
        }

        let entry_fee = ctx.accounts.lobby.entry_fee;

        // Transfer entry_fee from player → lobby PDA via system program CPI
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                    from: ctx.accounts.player.to_account_info(),
                    to: ctx.accounts.lobby.to_account_info(),
                },
            ),
            entry_fee,
        )?;

        let lobby = &mut ctx.accounts.lobby;
        let seat = lobby.player_count as usize;
        lobby.players[seat] = player_key;
        lobby.player_count += 1;
        lobby.pot += entry_fee;

        msg!(
            "Player {} joined lobby {} ({}/{}). Pot: {} lamports",
            player_key,
            lobby.lobby_id,
            lobby.player_count,
            lobby.max_players,
            lobby.pot
        );
        Ok(())
    }

    /// Player leaves the lobby before the match starts and gets a full refund.
    pub fn leave_lobby(ctx: Context<LeaveLobby>, _lobby_id: u64) -> Result<()> {
        let player_key = ctx.accounts.player.key();

        // Find player index
        let player_index = {
            let lobby = &ctx.accounts.lobby;
            require!(lobby.status == STATUS_OPEN, LobbyError::LobbyNotOpen);
            let mut found = None;
            for i in 0..lobby.player_count as usize {
                if lobby.players[i] == player_key {
                    found = Some(i);
                    break;
                }
            }
            found.ok_or(LobbyError::PlayerNotFound)?
        };

        let entry_fee = ctx.accounts.lobby.entry_fee;

        // Refund: lobby PDA → player (direct lamport manipulation, PDA is program-owned)
        **ctx
            .accounts
            .lobby
            .to_account_info()
            .try_borrow_mut_lamports()? -= entry_fee;
        **ctx
            .accounts
            .player
            .to_account_info()
            .try_borrow_mut_lamports()? += entry_fee;

        // Remove player from array (swap with last)
        let lobby = &mut ctx.accounts.lobby;
        let last = (lobby.player_count - 1) as usize;
        lobby.players[player_index] = lobby.players[last];
        lobby.players[last] = Pubkey::default();
        lobby.player_count -= 1;
        lobby.pot -= entry_fee;

        msg!(
            "Player {} left lobby {}. Pot: {} lamports",
            player_key,
            lobby.lobby_id,
            lobby.pot
        );
        Ok(())
    }

    /// Authority starts the match once enough players have joined.
    /// Records the BOLT entity PDA for cross-reference.
    pub fn start_match(
        ctx: Context<StartMatch>,
        _lobby_id: u64,
        match_entity: Pubkey,
    ) -> Result<()> {
        let lobby = &mut ctx.accounts.lobby;
        require!(
            lobby.authority == ctx.accounts.authority.key(),
            LobbyError::Unauthorized
        );
        require!(lobby.status == STATUS_OPEN, LobbyError::LobbyNotOpen);
        require!(
            lobby.player_count >= 2,
            LobbyError::NotEnoughPlayers
        );

        lobby.status = STATUS_STARTED;
        lobby.match_entity = match_entity;

        msg!(
            "Lobby {} → match started. Entity: {}, {} players, pot: {} lamports",
            lobby.lobby_id,
            match_entity,
            lobby.player_count,
            lobby.pot
        );
        Ok(())
    }

    /// Authority distributes the pot to one or more winners (split equally).
    /// winner_indices: seat indices in lobby.players[].
    /// Winner accounts must be passed as remaining_accounts in the same order, all writable.
    pub fn distribute_prize(
        ctx: Context<DistributePrize>,
        _lobby_id: u64,
        winner_indices: Vec<u8>,
    ) -> Result<()> {
        let lobby = &ctx.accounts.lobby;
        require!(lobby.status == STATUS_STARTED, LobbyError::MatchNotStarted);
        require!(!winner_indices.is_empty(), LobbyError::InvalidWinnerIndex);
        require!(
            winner_indices.len() == ctx.remaining_accounts.len(),
            LobbyError::WrongWinner
        );

        // Validate every index and matching account upfront
        for (&idx, acc) in winner_indices.iter().zip(ctx.remaining_accounts.iter()) {
            require!(
                (idx as usize) < lobby.player_count as usize,
                LobbyError::InvalidWinnerIndex
            );
            require!(
                lobby.players[idx as usize] == acc.key(),
                LobbyError::WrongWinner
            );
        }

        let pot = lobby.pot;
        let n = winner_indices.len() as u64;
        let share = pot / n;
        let remainder = pot % n; // goes to first winner

        let lobby_info = ctx.accounts.lobby.to_account_info();
        for (i, acc) in ctx.remaining_accounts.iter().enumerate() {
            let amount = if i == 0 { share + remainder } else { share };
            **lobby_info.try_borrow_mut_lamports()? -= amount;
            **acc.try_borrow_mut_lamports()? += amount;
        }

        let lobby = &mut ctx.accounts.lobby;
        lobby.status = STATUS_FINISHED;
        lobby.pot = 0;

        msg!(
            "Lobby {} finished. {} winner(s) split {} lamports ({} each)",
            lobby.lobby_id,
            n,
            pot,
            share
        );
        Ok(())
    }

    /// Authority cancels an open lobby. Players call leave_lobby individually
    /// to reclaim their deposits (status=Cancelled allows leave_lobby).
    pub fn cancel_lobby(ctx: Context<CancelLobby>, _lobby_id: u64) -> Result<()> {
        let lobby = &mut ctx.accounts.lobby;
        require!(
            lobby.authority == ctx.accounts.authority.key(),
            LobbyError::Unauthorized
        );
        // Allow cancellation if open OR started (before prize distributed)
        require!(
            lobby.status == STATUS_OPEN || lobby.status == STATUS_STARTED,
            LobbyError::LobbyNotOpen
        );

        lobby.status = STATUS_CANCELLED;
        msg!("Lobby {} cancelled", lobby.lobby_id);
        Ok(())
    }

    /// Player claims a refund after the lobby has been cancelled.
    pub fn claim_refund(ctx: Context<ClaimRefund>, _lobby_id: u64) -> Result<()> {
        let player_key = ctx.accounts.player.key();

        let player_index = {
            let lobby = &ctx.accounts.lobby;
            require!(
                lobby.status == STATUS_CANCELLED,
                LobbyError::LobbyNotOpen
            );
            let mut found = None;
            for i in 0..lobby.player_count as usize {
                if lobby.players[i] == player_key {
                    found = Some(i);
                    break;
                }
            }
            found.ok_or(LobbyError::PlayerNotFound)?
        };

        let entry_fee = ctx.accounts.lobby.entry_fee;

        **ctx
            .accounts
            .lobby
            .to_account_info()
            .try_borrow_mut_lamports()? -= entry_fee;
        **ctx
            .accounts
            .player
            .to_account_info()
            .try_borrow_mut_lamports()? += entry_fee;

        let lobby = &mut ctx.accounts.lobby;
        let last = (lobby.player_count - 1) as usize;
        lobby.players[player_index] = lobby.players[last];
        lobby.players[last] = Pubkey::default();
        lobby.player_count -= 1;
        lobby.pot -= entry_fee;

        msg!(
            "Refund: {} received {} lamports from cancelled lobby {}",
            player_key,
            entry_fee,
            lobby.lobby_id
        );
        Ok(())
    }
}

// ── Account Contexts ────────────────────────────────────────────────────────

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct CreateLobby<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = LOBBY_SIZE,
        seeds = [b"lobby", lobby_id.to_le_bytes().as_ref()],
        bump
    )]
    pub lobby: Account<'info, LobbyAccount>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct JoinLobby<'info> {
    #[account(mut)]
    pub player: Signer<'info>,

    #[account(
        mut,
        seeds = [b"lobby", lobby_id.to_le_bytes().as_ref()],
        bump = lobby.bump
    )]
    pub lobby: Account<'info, LobbyAccount>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct LeaveLobby<'info> {
    #[account(mut)]
    pub player: Signer<'info>,

    #[account(
        mut,
        seeds = [b"lobby", lobby_id.to_le_bytes().as_ref()],
        bump = lobby.bump
    )]
    pub lobby: Account<'info, LobbyAccount>,
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct StartMatch<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [b"lobby", lobby_id.to_le_bytes().as_ref()],
        bump = lobby.bump,
        has_one = authority @ LobbyError::Unauthorized
    )]
    pub lobby: Account<'info, LobbyAccount>,
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct DistributePrize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [b"lobby", lobby_id.to_le_bytes().as_ref()],
        bump = lobby.bump,
        has_one = authority @ LobbyError::Unauthorized
    )]
    pub lobby: Account<'info, LobbyAccount>,
    // winner accounts passed as remaining_accounts (all writable)
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct CancelLobby<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [b"lobby", lobby_id.to_le_bytes().as_ref()],
        bump = lobby.bump,
        has_one = authority @ LobbyError::Unauthorized
    )]
    pub lobby: Account<'info, LobbyAccount>,
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct ClaimRefund<'info> {
    #[account(mut)]
    pub player: Signer<'info>,

    #[account(
        mut,
        seeds = [b"lobby", lobby_id.to_le_bytes().as_ref()],
        bump = lobby.bump
    )]
    pub lobby: Account<'info, LobbyAccount>,
}
