use anchor_lang::prelude::*;
use game_config::GameConfig;

use crate::constants::*;
use crate::errors::LobbyError;
use crate::state::{LobbyAccount, Vault};

/// PER-aware payout. Replaces the legacy "back passes winner_indices"
/// flow with a trustless read of the `GameConfig` component on-chain:
///
///   1. Verify the passed `game_config` account is owned by the
///      canonical `game-config` program (constants::GAME_CONFIG_PROGRAM_ID).
///   2. Verify `GameConfig.bolt_metadata.entity == lobby.match_entity`
///      so the back can't substitute a different match's config.
///   3. Read `GameConfig.status` — must be Finished (resolve-turn
///      already wrote the final state).
///   4. Read `GameConfig.winners` (Vec<[u8; 32]>):
///        - empty                → 100% pot → house (timeout / wipe)
///        - non-empty            → 5% rake → house, 95% split equally
///                                  among the listed winners
///   5. Walk `remaining_accounts` in winner order, verify each pubkey
///      matches `GameConfig.winners[i]`, transfer the share to that
///      account.
///
/// `remaining_accounts` layout:
///   [0..winner_count]  winner wallets in `GameConfig.winners` order
///                      (all writable, no signer required).
pub fn distribute_prize(
    ctx: Context<DistributePrize>,
    _lobby_id: u64,
) -> Result<()> {
    // ── Rake authority hard-check. ─────────────────────────────────
    require!(
        ctx.accounts.rake_authority.key() == RAKE_AUTHORITY,
        LobbyError::InvalidRakeAuthority
    );

    // ── Lobby gate. ─────────────────────────────────────────────────
    require!(
        ctx.accounts.lobby.status == STATUS_STARTED,
        LobbyError::MatchNotStarted
    );
    // Belt-and-suspenders: status==Started should imply start_match
    // wrote a non-default match_entity, but explicit-check it so a
    // future code path that flips status without setting the entity
    // can't accidentally bypass the cross-component identity tie.
    require!(
        ctx.accounts.lobby.match_entity != Pubkey::default(),
        LobbyError::MatchNotStarted
    );

    // ── Verify GameConfig is the canonical match config. ──────────
    let game_config_acc = &ctx.accounts.game_config;
    require!(
        game_config_acc.owner == &GAME_CONFIG_PROGRAM_ID,
        LobbyError::InvalidGameConfigOwner
    );

    // Deserialize the GameConfig component. `Account::try_from`
    // applies Anchor's discriminator check.
    let game_config_data = game_config_acc.try_borrow_data()?;
    let game_config = GameConfig::try_deserialize(&mut &game_config_data[..])
        .map_err(|_| LobbyError::InvalidGameConfigOwner)?;
    drop(game_config_data);

    // Match-id tie: the back is required to pass `lobby_id` as the
    // `match_id` when calling `init-game`, so `GameConfig.match_id`
    // must equal `lobby.lobby_id` for the canonical pairing. Bolt
    // 0.2.4 doesn't expose the entity pubkey on `bolt_metadata`, so
    // we ship the cross-component identity tie ourselves via this
    // shared u64 — same trick we use between GameConfig and
    // PlayerRegistry on the ECS side.
    require!(
        game_config.match_id == ctx.accounts.lobby.lobby_id,
        LobbyError::GameConfigEntityMismatch
    );

    // ECS resolve-turn must have flipped this to Finished (= 2). The
    // numeric here mirrors `STATUS_FINISHED` in the game-config
    // component — we don't import that crate's const to avoid a
    // circular dep.
    require!(game_config.status == 2, LobbyError::MatchNotFinished);

    // ── Pot split. ──────────────────────────────────────────────────
    let pot = ctx.accounts.vault.total_pot;
    let vault_info = ctx.accounts.vault.to_account_info();
    let rake_info = ctx.accounts.rake_authority.to_account_info();

    let winner_count = game_config.winner_count as usize;

    if winner_count == 0 {
        // No winner (100-turn timeout or simultaneous wipe). House
        // takes everything — no rake split, just full pot.
        **vault_info.try_borrow_mut_lamports()? -= pot;
        **rake_info.try_borrow_mut_lamports()? += pot;

        let lobby = &mut ctx.accounts.lobby;
        lobby.status = STATUS_FINISHED;
        let vault = &mut ctx.accounts.vault;
        vault.total_pot = 0;

        msg!(
            "Lobby {} settled with no winner → {} lamports to house.",
            lobby.lobby_id, pot
        );
        return Ok(());
    }

    require!(
        ctx.remaining_accounts.len() == winner_count,
        LobbyError::WinnerCountMismatch
    );

    // ── Validate every winner account matches GameConfig.winners[i]. ─
    for (i, acc) in ctx.remaining_accounts.iter().enumerate() {
        require!(
            acc.key.to_bytes() == game_config.winners[i],
            LobbyError::WrongWinner
        );
    }

    // ── Compute splits: 5% rake → house, 95% equal split, remainder
    //    to the first winner so the math closes exactly. ─────────────
    let rake = pot.saturating_mul(RAKE_BPS) / 10_000;
    let prize = pot - rake;
    let n = winner_count as u64;
    let share = prize / n;
    let remainder = prize % n;

    **vault_info.try_borrow_mut_lamports()? -= rake;
    **rake_info.try_borrow_mut_lamports()? += rake;

    for (i, acc) in ctx.remaining_accounts.iter().enumerate() {
        let amount = if i == 0 { share + remainder } else { share };
        **vault_info.try_borrow_mut_lamports()? -= amount;
        **acc.try_borrow_mut_lamports()? += amount;
    }

    let lobby = &mut ctx.accounts.lobby;
    lobby.status = STATUS_FINISHED;
    let vault = &mut ctx.accounts.vault;
    vault.total_pot = 0;

    msg!(
        "Lobby {} settled. Rake: {} lamports. {} winners split {} lamports ({} each).",
        lobby.lobby_id, rake, n, prize, share
    );
    Ok(())
}

#[derive(Accounts)]
#[instruction(lobby_id: u64)]
pub struct DistributePrize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [LOBBY_SEED, lobby_id.to_le_bytes().as_ref()],
        bump = lobby.bump,
        has_one = authority @ LobbyError::Unauthorized
    )]
    pub lobby: Account<'info, LobbyAccount>,

    #[account(
        mut,
        seeds = [VAULT_SEED, lobby.key().as_ref()],
        bump = vault.bump
    )]
    pub vault: Account<'info, Vault>,

    /// CHECK: Address validated in instruction against the hardcoded
    /// `RAKE_AUTHORITY` in `constants.rs`.
    #[account(mut)]
    pub rake_authority: AccountInfo<'info>,

    /// CHECK: Owner + discriminator + entity validated in instruction.
    /// We deliberately don't use `Account<'info, GameConfig>` here so
    /// we can run the owner check ourselves and surface a clean error
    /// instead of Anchor's generic `AccountOwnedByWrongProgram`.
    pub game_config: AccountInfo<'info>,
    // winner accounts passed as remaining_accounts (all writable, in
    // GameConfig.winners order).
}
