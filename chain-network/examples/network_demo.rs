use chain_core::{Hash, BlockHeader, Transaction, Address};
use chain_network::{
    NetworkConfig, PeerIdentity, GossipManager, PeerManager, SyncManager,
    message::{BlockAnnounce, TransactionPropagate},
    transport::AddressFilter,
};
use tokio;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌐 Rust Blockchain Network Demo");
    println!("================================");

    // 1. 创建节点身份
    println!("1. 生成节点身份...");
    let identity = PeerIdentity::generate();
    let peer_id = identity.peer_id();
    println!("   节点 ID: {}", peer_id);    // 2. 创建网络配置
    println!("2. 配置网络参数...");
    let config = NetworkConfig::new();
    
    println!("   监听地址: {:?}", config.listen_addresses);
    println!("   最大对等节点: {}", config.max_peers);

    // 3. 创建 gossip 管理器
    println!("3. 初始化 Gossip 系统...");
    let (gossip_manager, mut _gossip_rx) = GossipManager::new();
    println!("   ✅ Gossip 消息系统已就绪");    // 4. 创建对等节点管理器
    println!("4. 初始化对等节点管理...");
    let _peer_manager = PeerManager::new(50, Duration::from_secs(30));
    println!("   ✅ 对等节点管理器已创建");

    // 5. 创建同步管理器
    println!("5. 初始化区块同步...");
    let (_sync_manager, _sync_rx) = SyncManager::new();
    println!("   ✅ 同步管理器已创建");

    // 6. 测试消息传播
    println!("6. 测试消息传播...");
    
    // 创建一个示例区块
    let header = BlockHeader::new(
        Hash::zero(),
        1,
        Hash::zero(),
        Hash::zero(),
        Hash::zero(),
        0,
        1719400000000, // 固定时间戳避免依赖 chrono
        vec![],
        0,
        21000,
        0,
    );
    
    let block_announce = BlockAnnounce::new(header);
    println!("   创建区块公告: 区块 #{}", block_announce.block_number());
    
    // 发布区块公告
    if let Err(e) = gossip_manager.announce_block(block_announce).await {
        println!("   ⚠️  发布失败: {}", e);
    } else {
        println!("   ✅ 区块公告已发送到网络");
    }

    // 7. 测试交易传播
    println!("7. 测试交易传播...");
    let tx = Transaction::new(
        0,                      // nonce
        1000000000,             // gas_price (1 gwei)
        21000,                  // gas_limit
        Some(Address::zero()),  // to
        1000000000000000000u128, // value (1 ETH)
        vec![],                 // data
    );
    
    let tx_propagate = TransactionPropagate::new(vec![tx]);
    if let Err(e) = gossip_manager.propagate_transactions(tx_propagate).await {
        println!("   ⚠️  传播失败: {}", e);
    } else {
        println!("   ✅ 交易已广播到网络");
    }    // 8. 展示网络统计
    println!("8. 网络统计信息:");
    println!("   已连接对等节点: 0 (演示模式)");
    println!("   同步状态: Ready (演示模式)");
    
    // 9. 测试地址过滤
    println!("9. 测试地址过滤...");
    let test_addresses = vec![
        "/ip4/127.0.0.1/tcp/30334".parse()?,
        "/ip4/8.8.8.8/tcp/30334".parse()?,
        "/ip4/192.168.1.100/tcp/30334".parse()?,
    ];
    
    let public_addresses = AddressFilter::get_public_addresses(test_addresses.clone());
    println!("   原始地址数量: {}", test_addresses.len());
    println!("   公共地址数量: {}", public_addresses.len());
    
    for addr in &public_addresses {
        println!("   公共地址: {}", addr);
    }

    println!("\n✅ 网络层功能演示完成!");
    println!("📋 已实现的功能:");
    println!("   • 节点身份生成和管理");
    println!("   • 网络配置系统");
    println!("   • Gossip 消息传播");
    println!("   • 对等节点管理");
    println!("   • 区块同步协调");
    println!("   • 地址过滤和验证");
    println!("   • 异步消息处理");

    Ok(())
}
