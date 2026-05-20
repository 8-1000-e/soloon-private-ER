use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::LobbyError;
use crate::state::{LobbyAccount, Vault};

/// Authority opens a fresh lobby at the given `lobby_id`. Creates both
/// the lobby metadata PDA and its escrow vault PDA in one tx. The
/// authority captured here is the only one allowed to call
/// start_match / cancel_lobby / distribute_prize / close_lobby.
pub fn create_lobby(
    ctx: Context<CreateLobby>,
    lobby_id: u64,
    entry_fee: u64,
    max_players: u8,
) -> Result<()> {
    require!(
        max_players as usize >= 2 && (max_players as usize) <= MAX_PLAYERS,
        LobbyError::InvalidMaxPlayers
    );
    // `leave_lobby` refunds `entry_fee - LEAVE_FEE_LAMPORTS`. If the
    // entry fee is too small, the refund saturates to 0 and the leaver
    // silently loses their full deposit. Require at least 2× the leave
    // fee so the refund stays meaningful and the leave fee can't ever
    // exceed the deposit.
    require!(
        entry_fee >= LEAVE_FEE_LAMPORTS.saturating_mul(2),
        LobbyError::EntryFeeTooLow
    );

    let now = Clock::get()?.unix_timestamp;

    let lobby = &mut ctx.accounts.lobby;
    lobby.lobby_id     = lobby_id;
    lobby.authority    = ctx.accounts.authority.key();
    lobby.entry_fee    = entry_fee;
    lobby.max_players  = max_players;
    lobby.player_count = 0;
    lobby.players      = [Pubkey::default(); MAX_PLAYERS];
    lobby.status       = STATUS_OPEN;
    lobby.match_entity = Pubkey::default();
    lobby.created_at   = now;
    lobby.started_at   = 0;
    lobby.bump         = ctx.bumps.lobby;

    let vault = &mut ctx.accounts.vault;
    vault.lobby     = lobby.key();
    vault.total_pot = 0;
    vault.bump      = ctx.bumps.vault;

    msg!(
        "Lobby {} open. entry_fee={} lamports, max_players={}",
        lobby_id, entry_fee, max_players
    );
    Ok(())
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct CreateLobby<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = LobbyAccount::LEN,
        seeds = [LOBBY_SEED, lobby_id.to_le_bytes().as_ref()],
        bump
    )]
    pub lobby: Account<'info, LobbyAccount>,

    #[account(
        init,
        payer = authority,
        space = Vault::LEN,
        seeds = [VAULT_SEED, lobby.key().as_ref()],
        bump
    )]
    pub vault: Account<'info, Vault>,

    pub system_program: Program<'info, System>,
}
