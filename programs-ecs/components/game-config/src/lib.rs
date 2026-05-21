use bolt_lang::*;
use ephemeral_rollups_sdk::access_control::structs::Member as SdkMember;
use ephemeral_rollups_sdk::consts::{MAGIC_PROGRAM_ID, PERMISSION_PROGRAM_ID};
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

// MagicVau1t999999999999999999999999999999999 — the ephemeral-permission
// vault PDA the TEE uses to hold rent for permission accounts on the ER.
// Pinned as a `const` because the SDK constant (`EPHEMERAL_VAULT_ID`) only
// landed in a post-0.8.5 git rev, and 0.8.5 is what bolt-lang transitively
// pins. Keep this address in lock-step with `EPHEMERAL_VAULT_ID` upstream.
// `MagicVau1t999999999999999999999999999999999` — bytes hard-coded because
// bolt-lang 0.2.4's `pubkey!` is a non-const fn, and the upstream SDK
// `EPHEMERAL_VAULT_ID` const only lives in a post-0.8.5 git rev.
pub const EPHEMERAL_VAULT_ID: Pubkey = Pubkey::new_from_array([
    5, 69, 180, 36, 224, 197, 24, 97, 240, 41, 76, 112, 66, 34, 84, 78,
    202, 127, 133, 79, 194, 135, 136, 166, 123, 118, 113, 80, 62, 224, 143, 184,
]);

// Discriminator for `create_ephemeral_permission` on the magicblock
// permission program. `u64` LE = 6, hand-rolled here because the
// `CreateEphemeralPermissionCpi` helper only ships in a post-0.8.5 git
// rev (see https://github.com/magicblock-labs/ephemeral-rollups-sdk/blob/
// d32189fe/rust/sdk/src/access_control/instructions/create_ephemeral_permission.rs).
// Adding the git dep would conflict with the 0.8.5 transitive pinned by
// bolt-lang 0.2.4, so we ship the wire-format manually.
const CREATE_EPHEMERAL_PERMISSION_DISCRIMINATOR: u64 = 6;

declare_id!("EcDbJA6zALpmc6te6r9CTpYHJgLYU9YhPJcTsMcPannB");

pub const MAX_PLAYERS: usize = 4;

#[account]
#[derive(InitSpace)]
pub struct GameConfig {
    pub match_id: u64,
    pub status: u8,
    pub phase: u8,
    pub turn: u16,
    pub active_players: u8,
    pub alive_count: u8,
    pub submitted_count: u8,
    pub turn_duration_secs: u16,
    pub turn_deadline: i64,
    pub min_players: u8,
    pub max_players: u8,
    pub protect_lock_turns: u8,
    pub bullets_cap: u8,
    pub mirrors_cap: u8,
    pub loot_mode: u8,
    pub win_mode: u8,
    #[max_len(MAX_PLAYERS)]
    pub winners: Vec<[u8; 32]>,
    pub winner_count: u8,
    #[max_len(MAX_PLAYERS)]
    pub pending_effects: Vec<PendingEffect>,
    pub apply_count: u8,
    pub bolt_metadata: BoltMetadata,
}

pub struct GameConfigInit {
    pub match_id: u64,
    pub status: u8,
    pub phase: u8,
    pub turn: u16,
    pub active_players: u8,
    pub alive_count: u8,
    pub submitted_count: u8,
    pub turn_duration_secs: u16,
    pub turn_deadline: i64,
    pub min_players: u8,
    pub max_players: u8,
    pub protect_lock_turns: u8,
    pub bullets_cap: u8,
    pub mirrors_cap: u8,
    pub loot_mode: u8,
    pub win_mode: u8,
    pub winners: Vec<[u8; 32]>,
    pub winner_count: u8,
    pub pending_effects: Vec<PendingEffect>,
    pub apply_count: u8,
}

impl GameConfig {
    pub fn new(init: GameConfigInit) -> Self {
        Self {
            match_id: init.match_id,
            status: init.status,
            phase: init.phase,
            turn: init.turn,
            active_players: init.active_players,
            alive_count: init.alive_count,
            submitted_count: init.submitted_count,
            turn_duration_secs: init.turn_duration_secs,
            turn_deadline: init.turn_deadline,
            min_players: init.min_players,
            max_players: init.max_players,
            protect_lock_turns: init.protect_lock_turns,
            bullets_cap: init.bullets_cap,
            mirrors_cap: init.mirrors_cap,
            loot_mode: init.loot_mode,
            win_mode: init.win_mode,
            winners: init.winners,
            winner_count: init.winner_count,
            pending_effects: init.pending_effects,
            apply_count: init.apply_count,
            bolt_metadata: BoltMetadata::default(),
        }
    }
}

#[automatically_derived]
impl ComponentTraits for GameConfig {
    fn seed() -> &'static [u8] {
        "".as_bytes()
    }

    fn size() -> usize {
        8 + <GameConfig>::INIT_SPACE
    }
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            match_id: 0,
            status: 0,
            phase: 0,
            turn: 0,
            active_players: 0,
            alive_count: 0,
            submitted_count: 0,
            turn_duration_secs: 0,
            turn_deadline: 0,
            min_players: 2,
            max_players: 4,
            protect_lock_turns: 1,
            bullets_cap: 6,
            mirrors_cap: 1,
            loot_mode: 0,
            win_mode: 0,
            winners: Vec::new(),
            winner_count: 0,
            pending_effects: vec![PendingEffect::default(); MAX_PLAYERS],
            apply_count: 0,
            bolt_metadata: BoltMetadata::default(),
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, InitSpace)]
pub struct PendingEffect {
    pub bullets: u8,
    pub mirrors: u8,
    pub hits_received: u8,
    pub protect_lock: u8,
    pub alive: bool,
    pub applied: bool,
}

impl Default for PendingEffect {
    fn default() -> Self {
        Self {
            bullets: 0,
            mirrors: 0,
            hits_received: 0,
            protect_lock: 0,
            alive: false,
            applied: false,
        }
    }
}

/// Argument shape passed to `init_permission`. Local copy of the SDK's
/// `Member` so the IDL stays free of SDK types.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MemberArg {
    pub flags: u8,
    pub pubkey: Pubkey,
}

#[delegate(GameConfig)]
#[bolt_program(GameConfig)]
pub mod game_config {
    use super::*;

    /// Create the EPHEMERAL permission account for THIS GameConfig PDA on
    /// the Private ER. Must be called ON THE ER (post-delegate), NOT on
    /// L1 — the TEE only consults ephemeral permissions that live on its
    /// own ledger. The component PDA signs via `invoke_signed` because the
    /// permission program enforces `permissioned_account.is_signer`.
    ///
    /// `is_private` is hard-coded to `true` — the whole point of this ix
    /// is to gate writes on the TEE. Members carry visibility flags
    /// (TX_LOGS / TX_MESSAGE / TX_BALANCES); the WRITE authority is
    /// implicit: only the owning component program can mutate the PDA via
    /// CPI from a Bolt system.
    pub fn init_permission(
        ctx: Context<InitPermission>,
        members: Vec<MemberArg>,
    ) -> Result<()> {
        let entity_key = ctx.accounts.entity.key();
        let bump = ctx.bumps.component;
        let pda_seeds: &[&[u8]] = &[
            <GameConfig as ComponentTraits>::seed(),
            entity_key.as_ref(),
            std::slice::from_ref(&bump),
        ];

        // Wire-format `EphemeralMembersArgs`: `is_private(1) || N×(flags(1)+pubkey(32))`.
        let mut args_bytes = Vec::with_capacity(1 + members.len() * 33);
        args_bytes.push(1u8); // is_private = true
        for m in &members {
            args_bytes.push(m.flags);
            args_bytes.extend_from_slice(m.pubkey.as_ref());
        }

        let mut data = Vec::with_capacity(8 + args_bytes.len());
        data.extend_from_slice(&CREATE_EPHEMERAL_PERMISSION_DISCRIMINATOR.to_le_bytes());
        data.extend_from_slice(&args_bytes);

        // Accounts mirror the SDK's CreateEphemeralPermission. The CPI
        // `payer` IS the component PDA — it must be a delegated account
        // so the TEE allows lamports to be debited (the wallet is not
        // delegated, so the TEE would reject any rent withdrawal from
        // it with "Feepayer was modified without being delegated").
        // The back pre-funds each component PDA with permission rent
        // on L1 before delegate.
        let accounts = vec![
            AccountMeta::new(ctx.accounts.component.key(), true),
            AccountMeta::new_readonly(ctx.accounts.component.key(), true),
            AccountMeta::new(ctx.accounts.permission.key(), false),
            AccountMeta::new(ctx.accounts.ephemeral_vault.key(), false),
            AccountMeta::new_readonly(ctx.accounts.magic_program.key(), false),
        ];
        let ix = Instruction {
            program_id: PERMISSION_PROGRAM_ID,
            accounts,
            data,
        };
        invoke_signed(
            &ix,
            &[
                ctx.accounts.component.to_account_info(),
                ctx.accounts.permission.to_account_info(),
                ctx.accounts.ephemeral_vault.to_account_info(),
                ctx.accounts.magic_program.to_account_info(),
            ],
            &[pda_seeds],
        )?;

        // Use SdkMember to keep a unused-import lint quiet — borrow checker
        // dropped the type otherwise. Members already serialized above.
        let _silence: Vec<SdkMember> = Vec::new();
        Ok(())
    }

    #[derive(Accounts)]
    pub struct InitPermission<'info> {
        #[account(mut)]
        pub payer: Signer<'info>,
        pub entity: Account<'info, Entity>,
        /// Component PDA — owner is the delegation program post-delegate, so
        /// we use `UncheckedAccount` here; the seeds+bump constraint pins
        /// the address and lets us sign via `invoke_signed`.
        #[account(
            mut,
            seeds = [<GameConfig as ComponentTraits>::seed(), entity.key().as_ref()],
            bump,
        )]
        /// CHECK: PDA verified by seeds + bump; mutation gated by the CPI signer.
        pub component: UncheckedAccount<'info>,
        /// CHECK: validated by the permission-program CPI.
        #[account(mut)]
        pub permission: AccountInfo<'info>,
        /// CHECK: address-pinned to the magicblock ephemeral vault.
        #[account(mut, address = EPHEMERAL_VAULT_ID)]
        pub ephemeral_vault: UncheckedAccount<'info>,
        /// CHECK: address-pinned to the magicblock magic program.
        #[account(address = MAGIC_PROGRAM_ID)]
        pub magic_program: UncheckedAccount<'info>,
        /// CHECK: address-pinned to the magicblock permission program.
        #[account(address = PERMISSION_PROGRAM_ID)]
        pub permission_program: UncheckedAccount<'info>,
        pub system_program: Program<'info, System>,
    }
}
