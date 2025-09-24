use rand::RngCore;
use sp_core::H256 as Hash;
use substrate_api_client::{
	ac_primitives::DefaultRuntimeConfig, rpc::JsonrpseeClient, Api, SubscribeEvents,
};
use midnight_node_runtime::RuntimeEvent;


pub fn new_dust_hex(bytes: usize) -> String {
	let mut a = vec![0u8; bytes];
	rand::rng().fill_bytes(&mut a);
	a.iter().map(|b| format!("{:02x}", b)).collect::<String>()
}

pub async fn subscribe_to_events() {
	let client = JsonrpseeClient::new_with_port("ws://127.0.0.1", 9933).await.unwrap();
	let api = Api::<DefaultRuntimeConfig, _>::new(client).await.unwrap();

	println!("Subscribe to events");
	let mut subscription = api.subscribe_events().await.unwrap();

	for _ in 0..5 {
		let event_records =
			subscription.next_events::<RuntimeEvent, Hash>().await.unwrap().unwrap();
		for event_record in &event_records {
			println!("decoded: {:?}", event_record);
		}
	}
	subscription.unsubscribe().await.unwrap();
}