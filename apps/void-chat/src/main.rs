use anyhow::Result;
use void_identity::NodeIdentity;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    let identity = NodeIdentity::generate();
    println!("VOIDChat runtime shell");
    println!("peer_id={}", identity.peer_id());
    println!("rooms=decentralized");
    println!("message_transport=VOID Protocol peer frames");
    println!("encryption=planned aes-gcm payload layer");

    Ok(())
}
