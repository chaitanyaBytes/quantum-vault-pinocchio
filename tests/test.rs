use litesvm::LiteSVM;
use solana_sdk::{
    message::{AccountMeta, Instruction},
    native_token::LAMPORTS_PER_SOL,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
};
use solana_system_interface::program;
use solana_winternitz::privkey::WinternitzPrivkey;
use std::str::FromStr;

#[test]
pub fn test_quantum_vault_refund() {
    let mut svm = LiteSVM::new();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10 * LAMPORTS_PER_SOL)
        .expect("failed to airdrop");

    let program_id_bytes: [u8; 32] = [
        0x0f, 0x1e, 0x6b, 0x14, 0x21, 0xc0, 0x4a, 0x07, 0x04, 0x31, 0x26, 0x5c, 0x19, 0xc5, 0xbb,
        0xee, 0x19, 0x92, 0xba, 0xe8, 0xaf, 0xd1, 0xcd, 0x07, 0x8e, 0xf8, 0xaf, 0x70, 0x47, 0xdc,
        0x11, 0xf7,
    ];
    let program_id = Pubkey::from(program_id_bytes);
    let program_bytes = include_bytes!("../../target/deploy/quantum_vault_pinocchio.so");

    svm.add_program(program_id, program_bytes)
        .expect("failed to add program");

    let vault_keypair = WinternitzPrivkey::generate();
    let vault_pubkey_hash = vault_keypair.pubkey().merklize();

    // Find PDA for the vault
    let (vault_address, bump) =
        Pubkey::find_program_address(&[vault_pubkey_hash.as_ref()], &program_id);

    // 1. Test open instruction
    let mut open_ix_data = vec![0u8]; // Discriminator
    open_ix_data.extend_from_slice(&vault_pubkey_hash);
    open_ix_data.push(bump);

    let open_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(vault_address, false),
            AccountMeta::new_readonly(program::ID, false),
        ],
        data: open_ix_data,
    };

    let tx = Transaction::new_signed_with_payer(
        &[open_ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );

    let result = svm.send_transaction(tx);
    if let Ok(response) = &result {
        let logs = response.pretty_logs();
        println!("open transaction logs:\n{}", logs);
    } else if let Err(e) = &result {
        eprintln!("open transaction failed: {:?}", e);
    }
    result.expect("Failed to open vault");

    // 2. Fund the vault
    let transfer_ix = Instruction {
        program_id: program::ID,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(vault_address, false),
        ],
        data: {
            let mut data = vec![2, 0, 0, 0]; // Transfer instruction discriminator
            data.extend_from_slice(&(5 * LAMPORTS_PER_SOL).to_le_bytes());
            data
        },
    };

    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );
    let result = svm.send_transaction(tx);
    if let Ok(response) = &result {
        let logs = response.pretty_logs();
        println!("fund transaction logs:\n{}", logs);
    } else if let Err(e) = &result {
        eprintln!("fund transaction failed: {:?}", e);
    }
    result.expect("Failed to fund vault");

    let vault_account = svm.get_account(&vault_address).unwrap();
    assert_eq!(vault_account.lamports, 5 * LAMPORTS_PER_SOL + 890880);

    // 3. Test split instruction
    let split_account = Keypair::new();
    let refund_account = Keypair::new();
    let split_amount = 2 * LAMPORTS_PER_SOL;

    // Build the 72-byte message: [amount (8 bytes) | split_pubkey (32 bytes) | refund_pubkey (32 bytes)]
    let mut message = [0u8; 72];
    message[0..8].copy_from_slice(&split_amount.to_le_bytes());
    message[8..40].copy_from_slice(split_account.pubkey().as_ref());
    message[40..72].copy_from_slice(refund_account.pubkey().as_ref());

    // Sign the message with Winternitz private key
    let signature = vault_keypair.sign(&message);
    let signature_bytes: [u8; 896] = signature.into();

    let mut split_ix_data = vec![1u8];
    split_ix_data.extend_from_slice(&signature_bytes);
    split_ix_data.push(bump);
    split_ix_data.extend_from_slice(&split_amount.to_le_bytes());

    let split_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(vault_address, false),
            AccountMeta::new(split_account.pubkey(), false),
            AccountMeta::new(refund_account.pubkey(), false),
        ],
        data: split_ix_data,
    };

    // add compute budget instruction for Winternitz signature verification
    let compute_budget_ix = Instruction {
        program_id: Pubkey::from_str("ComputeBudget111111111111111111111111111111").unwrap(),
        accounts: vec![],
        data: {
            let mut data = vec![2, 0, 0, 0]; // SetComputeUnitLimit instruction discriminator
            data.extend_from_slice(&1_400_000u32.to_le_bytes());
            data
        },
    };

    let tx = Transaction::new_signed_with_payer(
        &[compute_budget_ix, split_ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );

    let result = svm.send_transaction(tx);
    if let Ok(response) = &result {
        let logs = response.pretty_logs();
        println!("Split transaction logs:\n{}", logs);
    } else if let Err(e) = &result {
        eprintln!("Split transaction failed: {:?}", e);
    }
    result.expect("Failed to split vault");

    let split_account_info = svm.get_account(&split_account.pubkey()).unwrap();
    assert_eq!(split_account_info.lamports, split_amount);

    let refund_account_info = svm.get_account(&refund_account.pubkey()).unwrap();
    let split_account_info = svm.get_account(&split_account.pubkey()).unwrap();
    assert_eq!(
        refund_account_info.lamports,
        5 * LAMPORTS_PER_SOL - split_amount + 890880
    );

    assert_eq!(split_account_info.lamports, split_amount);

    let vault_account_after = svm.get_account(&vault_address);
    assert!(vault_account_after.is_none() || vault_account_after.unwrap().lamports == 0);
}

#[test]
pub fn test_quantum_vault_close() {
    let mut svm = LiteSVM::new();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10 * LAMPORTS_PER_SOL)
        .expect("failed to airdrop");

    let program_id_bytes: [u8; 32] = [
        0x0f, 0x1e, 0x6b, 0x14, 0x21, 0xc0, 0x4a, 0x07, 0x04, 0x31, 0x26, 0x5c, 0x19, 0xc5, 0xbb,
        0xee, 0x19, 0x92, 0xba, 0xe8, 0xaf, 0xd1, 0xcd, 0x07, 0x8e, 0xf8, 0xaf, 0x70, 0x47, 0xdc,
        0x11, 0xf7,
    ];
    let program_id = Pubkey::from(program_id_bytes);
    let program_bytes = include_bytes!("../../target/deploy/quantum_vault_pinocchio.so");

    svm.add_program(program_id, program_bytes)
        .expect("failed to add program");

    let vault_keypair = WinternitzPrivkey::generate();
    let vault_pubkey_hash = vault_keypair.pubkey().merklize();

    let (vault_address, bump) =
        Pubkey::find_program_address(&[vault_pubkey_hash.as_ref()], &program_id);

    // 1. Open vault
    let mut open_ix_data = vec![0u8];
    open_ix_data.extend_from_slice(&vault_pubkey_hash);
    open_ix_data.push(bump);

    let open_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(vault_address, false),
            AccountMeta::new_readonly(program::ID, false),
        ],
        data: open_ix_data,
    };

    let tx = Transaction::new_signed_with_payer(
        &[open_ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );

    let result = svm.send_transaction(tx);
    if let Ok(response) = &result {
        let logs = response.pretty_logs();
        println!("open transaction logs:\n{}", logs);
    } else if let Err(e) = &result {
        eprintln!("open transaction failed: {:?}", e);
    }
    result.expect("Failed to open vault");

    // 2. Fund the vault
    let transfer_ix = Instruction {
        program_id: program::ID,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(vault_address, false),
        ],
        data: {
            let mut data = vec![2, 0, 0, 0]; // Transfer instruction discriminator
            data.extend_from_slice(&(3 * LAMPORTS_PER_SOL).to_le_bytes());
            data
        },
    };
    let tx = Transaction::new_signed_with_payer(
        &[transfer_ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );

    let result = svm.send_transaction(tx);
    if let Ok(response) = &result {
        let logs = response.pretty_logs();
        println!("fund transaction logs:\n{}", logs);
    } else if let Err(e) = &result {
        eprintln!("fund transaction failed: {:?}", e);
    }
    result.expect("Failed to fund the vault");

    // close instruction
    let refund_account = Keypair::new();

    let signature = vault_keypair.sign(refund_account.pubkey().as_ref());
    let signature_bytes: [u8; 896] = signature.into();

    let mut close_ix_data = vec![2u8]; // Discriminator
    close_ix_data.extend_from_slice(&signature_bytes);
    close_ix_data.push(bump);

    let close_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(vault_address, false),
            AccountMeta::new(refund_account.pubkey(), false),
        ],
        data: close_ix_data,
    };

    // Add compute budget instruction for Winternitz signature verification
    let compute_budget_ix = Instruction {
        program_id: Pubkey::from_str("ComputeBudget111111111111111111111111111111").unwrap(),
        accounts: vec![],
        data: {
            let mut data = vec![2, 0, 0, 0]; // SetComputeUnitLimit instruction discriminator
            data.extend_from_slice(&1_400_000u32.to_le_bytes());
            data
        },
    };

    let tx = Transaction::new_signed_with_payer(
        &[compute_budget_ix, close_ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );

    let result = svm.send_transaction(tx);
    if let Ok(response) = &result {
        let logs = response.pretty_logs();
        println!("Close transaction logs:\n{}", logs);
    } else if let Err(e) = &result {
        eprintln!("Close transaction failed: {:?}", e);
    }
    result.expect("Failed to close vault");

    let refund_account_info = svm.get_account(&refund_account.pubkey()).unwrap();
    assert_eq!(refund_account_info.lamports, 3 * LAMPORTS_PER_SOL + 890880);

    let vault_account_after = svm.get_account(&vault_address);
    assert!(vault_account_after.is_none() || vault_account_after.unwrap().lamports == 0);
}
