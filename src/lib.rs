pub mod instructions;
use pinocchio::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, ProgramResult,
};

pub use instructions::*;

#[cfg(not(feature = "no-entrypoint"))]
use pinocchio::entrypoint;

use crate::instructions::{close::CloseVault, open::OpenVault, split::SplitVault};

#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);

pub const ID: Pubkey = [
    0x0f, 0x1e, 0x6b, 0x14, 0x21, 0xc0, 0x4a, 0x07, 0x04, 0x31, 0x26, 0x5c, 0x19, 0xc5, 0xbb, 0xee,
    0x19, 0x92, 0xba, 0xe8, 0xaf, 0xd1, 0xcd, 0x07, 0x8e, 0xf8, 0xaf, 0x70, 0x47, 0xdc, 0x11, 0xf7,
];

pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    match instruction_data.split_first() {
        Some((OpenVault::DISCRIMINATOR, data)) => OpenVault::try_from((data, accounts))?.process(),
        Some((SplitVault::DISCRIMINATOR, data)) => {
            SplitVault::try_from((data, accounts))?.process()
        }
        Some((CloseVault::DISCRIMINATOR, data)) => {
            CloseVault::try_from((data, accounts))?.process()
        }
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
