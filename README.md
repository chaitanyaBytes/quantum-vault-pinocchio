# Quantum Vault Pinocchio

A Solana program implementing a quantum-safe vault using Winternitz one-time signatures. Unlike traditional cryptography, Winternitz signatures become vulnerable after a single use, making them ideal for one-time payment scenarios.

## Overview

This program provides a quantum-resistant vault system where:

- Vaults are controlled by Winternitz keypairs (quantum-safe)
- Each signature can only be used once
- The vault address is derived from a hash of the Winternitz public key
- Funds can be split to multiple recipients or closed entirely

## Architecture

### Winternitz Signatures

Winternitz signatures are a post-quantum cryptographic scheme that provides security against quantum computers. However, they have a critical property: **each signature can only be used once**. After signing, the private key material is partially revealed, making subsequent signatures insecure.

### Vault Derivation

Vaults are Program Derived Addresses (PDAs) created using:

- Seed: SHA-256 hash of the Winternitz public key (after merklization)
- Bump: The canonical bump seed for the PDA

The vault address is computed as:

```
SHA256(hash || bump || program_id || "ProgramDerivedAddress")
```

## Instructions

### 1. Open Vault (Discriminator: 0)

Creates a new quantum vault account.

**Accounts:**

- `payer` (signer, writable): Account paying for vault creation
- `vault` (writable): The vault PDA account to create
- `system_program` (readonly): System program

**Instruction Data:**

- `hash`: 32-byte SHA-256 hash of the Winternitz public key (merklized)
- `bump`: 1-byte PDA derivation bump

**Process:**

1. Creates a new account owned by the program
2. Uses the hash and bump as PDA seeds
3. Account is initialized with minimum rent-exempt balance

### 2. Split Vault (Discriminator: 1)

Splits vault funds between a split account and a refund account, then closes the vault.

**Accounts:**

- `vault` (writable): Source vault account
- `split` (writable): Recipient account for the specified amount
- `refund` (writable): Recipient account for remaining balance

**Instruction Data:**

- `signature`: 896-byte Winternitz signature
- `bump`: 1-byte PDA derivation bump
- `amount`: 8-byte little-endian amount in lamports

**Message Format:**
The signature is over a 72-byte message:

- Bytes 0-7: Amount to split (u64, little-endian)
- Bytes 8-39: Split account public key (32 bytes)
- Bytes 40-71: Refund account public key (32 bytes)

**Process:**

1. Assembles the 72-byte message from amount and account pubkeys
2. Recovers the Winternitz public key from the signature
3. Merklizes the recovered pubkey to get the hash
4. Verifies the hash matches the vault PDA seeds
5. Transfers the specified amount to the split account
6. Transfers remaining balance to the refund account
7. Closes the vault account

**Note:** The refund account can be another quantum vault, allowing you to roll over funds to a new vault with a fresh keypair.

### 3. Close Vault (Discriminator: 2)

Closes the vault and sends all funds to a refund account.

**Accounts:**

- `vault` (writable): Vault account to close
- `refund` (writable): Recipient account for all funds

**Instruction Data:**

- `signature`: 896-byte Winternitz signature
- `bump`: 1-byte PDA derivation bump

**Message Format:**
The signature is over the refund account's public key (32 bytes).

**Process:**

1. Recovers the Winternitz public key from the signature
2. Merklizes the recovered pubkey to get the hash
3. Verifies the hash matches the vault PDA seeds
4. Transfers all vault lamports to the refund account
5. Closes the vault account

## Compute Unit Requirements

**Important:** Winternitz signature verification is computationally expensive. Transactions that use `split` or `close` instructions require significantly more compute units than the default limit.

### Default Compute Budget

- Default: 200,000 compute units
- Required for split/close: ~500,000-600,000+ compute units

### Setting Compute Budget

When calling `split` or `close` instructions, you must include a compute budget instruction to increase the compute unit limit:

```rust
use solana_compute_budget_instruction::compute_budget;

// Set compute unit limit to 1,400,000 (sufficient for Winternitz verification)
let compute_budget_ix = compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);

// Include it as the first instruction in your transaction
let tx = Transaction::new_signed_with_payer(
    &[compute_budget_ix, split_ix], // or close_ix
    Some(&payer.pubkey()),
    &[&payer],
    latest_blockhash,
);
```

Alternatively, you can manually construct the compute budget instruction:

```rust
let compute_budget_ix = Instruction {
    program_id: Pubkey::from_str("ComputeBudget111111111111111111111111111111").unwrap(),
    accounts: vec![],
    data: {
        let mut data = vec![2, 0, 0, 0]; // SetComputeUnitLimit discriminator
        data.extend_from_slice(&1_400_000u32.to_le_bytes());
        data
    },
};
```

### Why Extra Compute Units Are Needed

Winternitz signature verification involves:

1. Recovering the public key from the signature (896 bytes)
2. Merklizing the recovered public key
3. Computing SHA-256 hashes for verification
4. PDA equivalence checking

These operations consume approximately 500,000-600,000 compute units, which exceeds Solana's default transaction compute budget of 200,000 units.

## Building

```bash
cargo build-sbf
```

The compiled program will be in `target/deploy/quantum_vault_pinocchio.so`.

## Testing

Run the test suite:

```bash
cargo test -p quantum-vault-pinocchio -- --show-output
```

The tests demonstrate:

- Creating a vault with a Winternitz keypair
- Funding the vault
- Splitting funds to multiple accounts
- Closing the vault and refunding all funds

### Test Structure

1. Generate a Winternitz keypair
2. Compute the merklized hash of the public key
3. Derive the vault PDA using the hash as a seed
4. Create and fund the vault
5. Sign messages with the Winternitz private key
6. Execute split or close instructions with proper compute budget

## Key Concepts

### Merklization

The program uses `.merklize()` on Winternitz public keys to create a compact 32-byte hash. This hash is used as the PDA seed. When verifying signatures, the program recovers the pubkey and merklizes it again to get the same hash for verification.

### One-Time Use Property

Winternitz signatures can only be used once. After signing a message, parts of the private key are revealed. This makes them perfect for:

- One-time payments
- Vault operations where each action consumes the signature
- Scenarios where quantum resistance is required

### Signature Format

Winternitz signatures are 896 bytes. They are included directly in the instruction data (not as transaction signatures) because the signature itself proves authority - it's not a byproduct of the transaction, but rather the transaction's authority.
