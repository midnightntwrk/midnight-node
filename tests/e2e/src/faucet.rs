use crate::api::cardano::CardanoClient;
use crate::config::OgmiosClientSettings;
use ogmios_client::types::OgmiosUtxo;
use tokio::sync::Mutex;
use whisky::Asset;

pub struct FaucetManager {
    pub ogmios_settings: OgmiosClientSettings,
    pub faucet: CardanoClient,
    manager_utxo: Mutex<OgmiosUtxo>,
    locked_utxos: Mutex<Vec<OgmiosUtxo>>,
    utxo_lock: Mutex<()>,
}

impl FaucetManager {
    pub async fn new(ogmios_settings: OgmiosClientSettings, faucet: CardanoClient) -> Self {
        let manager_utxo = faucet
            .utxo_with_max_lovelace()
            .await
            .expect("Faucet has no UTXOs");

        FaucetManager {
            ogmios_settings: ogmios_settings.clone(),
            faucet,
            manager_utxo: Mutex::new(manager_utxo),
            locked_utxos: Mutex::new(Vec::new()),
            utxo_lock: Mutex::new(()),
        }
    }

    pub async fn request_tokens(&self, address: &str, lovelace: u64) -> OgmiosUtxo {
        let tx_in = Self::lock_utxo(self, lovelace)
            .await
            .expect("Failed to lock UTXO for faucet request");
        let assets = vec![Asset::new_from_str("lovelace", &lovelace.to_string())];
        self.faucet
            .fund_wallet(&tx_in, address, assets)
            .await
            .expect("Failed to request tokens from faucet")
    }

    fn utxo(u: &OgmiosUtxo) -> ([u8; 32], u16) {
        (u.transaction.id, u.index)
    }

    async fn lock_utxo(&self, lovelace: u64) -> Option<OgmiosUtxo> {
        self.lock_utxo_with_fee(lovelace, 10_000_000).await
    }

    async fn lock_utxo_with_fee(&self, lovelace: u64, fee: u64) -> Option<OgmiosUtxo> {
        let _guard = self.utxo_lock.lock().await;
        let expected_lovelace = lovelace + fee;
        let manager_utxo = self.manager_utxo.lock().await.clone();
        let locked = self.locked_utxos.lock().await;
        let locked_utxos: Vec<_> = locked.iter().map(Self::utxo).collect();
        drop(locked);

        // pick eligible UTXO
        let utxos = self.faucet.utxos().await;
        let utxo = utxos
            .into_iter()
            .filter(|u| {
                let id = Self::utxo(u);
                id != Self::utxo(&manager_utxo)
                    && !locked_utxos.contains(&id)
                    && u.value.lovelace >= expected_lovelace
            })
            .max_by_key(|u| u.value.lovelace);

        match utxo {
            Some(u) => {
                self.locked_utxos.lock().await.push(u.clone());
                Some(u)
            }
            None => {
                println!(
                    "No eligible UTXO found with {} lovelace. Creating new one...",
                    expected_lovelace
                );

                let assets = vec![Asset::new_from_str(
                    "lovelace",
                    &expected_lovelace.to_string(),
                )];
                let new_utxo = self
                    .faucet
                    .fund_wallet(&manager_utxo, &self.faucet.address_as_bech32(), assets)
                    .await
                    .unwrap();

                self.locked_utxos.lock().await.push(new_utxo.clone());

                let new_manager = self.faucet.utxo_with_max_lovelace().await.unwrap();

                *self.manager_utxo.lock().await = new_manager;

                Some(new_utxo)
            }
        }
    }
}
