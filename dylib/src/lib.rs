use reth_exex::{ExExEvent, ExExNotification};

#[stabby::export(canaries)]
pub extern "C" fn greet() {
    println!("gm");
}

#[stabby::export(canaries)]
pub async extern "C" fn process_notification(data: &[u8]) -> eyre::Result<Option<ExExEvent>> {
    let notification: ExExNotification = serde_json::from_slice(data)?;

    match &notification {
        ExExNotification::ChainCommitted { new } => {
            println!("Received commit: {:?}", new.range());
        }
        ExExNotification::ChainReorged { old, new } => {
            println!("Received reorg: {:?} {:?}", old.range(), new.range());
        }
        ExExNotification::ChainReverted { old } => {
            println!("Received revert: {:?}", old.range());
        }
    };

    if let Some(committed_chain) = notification.committed_chain() {
        Ok(Some(ExExEvent::FinishedHeight(committed_chain.tip().number)))
    } else {
        Ok(None)
    }
}
