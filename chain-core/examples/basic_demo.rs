//! Example demonstrating basic blockchain functionality

use core::{Address, Block, BlockHeader, Hash, KeccakPatriciaTrie, Transaction, Trie};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🦀 Rust Blockchain Core Demo");
    println!("============================");

    // Create basic types
    println!("\n1. Creating basic types...");
    let genesis_hash = Hash::zero();
    let alice_addr = Address::from_hex("1234567890abcdef1234567890abcdef12345678")?;
    let bob_addr = Address::from_hex("abcdef1234567890abcdef1234567890abcdef12")?;

    println!("   Genesis hash: {}", genesis_hash);
    println!("   Alice address: {}", alice_addr);
    println!("   Bob address: {}", bob_addr);

    // Create a simple transaction
    println!("\n2. Creating a transaction...");
    let tx = Transaction::transfer(
        1,              // nonce
        bob_addr,       // to
        1000,           // value (1000 wei)
        20_000_000_000, // gas price
        21_000,         // gas limit
    );

    let tx_hash = tx.hash()?;
    println!("   Transaction hash: {}", tx_hash);
    println!("   Transfer: {} wei from Alice to Bob", tx.value);

    // Create a block header
    println!("\n3. Creating a block header...");
    let header = BlockHeader::new(
        genesis_hash,                   // parent hash
        1,                              // block number
        Hash::zero(),                   // state root
        Hash::zero(),                   // transactions root
        Hash::zero(),                   // receipts root
        1000,                           // difficulty
        1640995200000,                  // timestamp (2022-01-01)
        b"Rust Chain Block 1".to_vec(), // extra data
        42,                             // nonce
        8_000_000,                      // gas limit
        21_000,                         // gas used
    );

    let block_hash = header.hash()?;
    println!("   Block hash: {}", block_hash);
    println!("   Block number: {}", header.number);

    // Create a complete block
    println!("\n4. Creating a complete block...");
    let block = Block::new(header, vec![tx]);
    println!(
        "   Block contains {} transactions",
        block.transactions.len()
    );
    println!("   Total gas used: {}", block.total_gas_used());

    // Demonstrate trie functionality
    println!("\n5. Testing trie functionality...");
    let mut trie = KeccakPatriciaTrie::new();

    // Insert some state data
    trie.insert(
        alice_addr.as_bytes(),
        bincode::encode_to_vec(&1000u64, bincode::config::standard())?,
    )?;
    trie.insert(
        bob_addr.as_bytes(),
        bincode::encode_to_vec(&500u64, bincode::config::standard())?,
    )?;

    let trie_root = trie.root_hash();
    println!("   Trie root hash: {}", trie_root);

    // Retrieve Alice's balance
    if let Some(alice_balance_bytes) = trie.get(alice_addr.as_bytes())? {
        let (alice_balance, _): (u64, usize) =
            bincode::decode_from_slice(&alice_balance_bytes, bincode::config::standard())?;
        println!("   Alice's balance: {} wei", alice_balance);
    }

    println!("\n✅ All operations completed successfully!");
    println!("   - Basic types: Hash, Address ✓");
    println!("   - Transactions: Creation, hashing ✓");
    println!("   - Blocks: Headers, complete blocks ✓");
    println!("   - Trie: State storage and retrieval ✓");

    Ok(())
}
