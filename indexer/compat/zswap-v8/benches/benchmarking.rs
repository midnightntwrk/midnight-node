#![deny(warnings)]

use base_crypto::data_provider::{self, MidnightDataProvider};
use base_crypto::rng::SplittableRng;
use coin_structure::coin::Info as CoinInfo;
use coin_structure::coin::ShieldedTokenType;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use midnight_zswap::keys::SecretKeys;
use midnight_zswap::ledger::State as ZswapLedgerState;
use midnight_zswap::local::State as ZswapLocalState;
use midnight_zswap::prove::ZswapResolver;
use midnight_zswap::{Delta, Offer, Output as ZswapOutput, ZSWAP_EXPECTED_FILES};
use rand::SeedableRng;
use rand::rngs::OsRng;
use rand::rngs::StdRng;
use rand::{CryptoRng, Rng};
use storage::db::InMemoryDB;
use transient_crypto::proofs::{
    ParamsProverProvider, Proof, ProofPreimage, ProvingError, Resolver,
};
use zkir_v2::LocalProvingProvider;

fn sync_prove(
    offer: &Offer<ProofPreimage, InMemoryDB>,
    rng: impl CryptoRng + SplittableRng,
    pp: &impl ParamsProverProvider,
    resolver: &impl Resolver,
) -> Result<Offer<Proof, InMemoryDB>, ProvingError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let provider = LocalProvingProvider {
        rng,
        params: pp,
        resolver,
    };

    rt.block_on(async {
        let (_, o) = offer.clone().prove(provider, 0).await?;
        Ok(o)
    })
}

pub fn zswap_ledger(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0x42);
    let mut zswap_local_state = ZswapLocalState::<InMemoryDB>::new();
    let keys = SecretKeys::from_rng_seed(&mut rng);
    let zswap_state: ZswapLedgerState<InMemoryDB> = ZswapLedgerState::new();
    let coin = CoinInfo {
        nonce: OsRng.r#gen(),
        type_: ShieldedTokenType(OsRng.r#gen()),
        value: OsRng.r#gen::<u64>() as u128,
    };
    let resolver = ZswapResolver(
        MidnightDataProvider::new(
            data_provider::FetchMode::Synchronous,
            data_provider::OutputMode::Log,
            ZSWAP_EXPECTED_FILES.to_owned(),
        )
        .unwrap(),
    );
    let output = ZswapOutput::new(&mut rng, &coin, None, &keys.coin_public_key(), None).unwrap();
    let offer = Offer {
        inputs: storage::storage::Array::new(),
        outputs: vec![output].into(),
        transient: storage::storage::Array::new(),
        deltas: storage::storage::Array::new(),
    };
    c.bench_function("ledger::application", |b| {
        b.iter(|| zswap_state.try_apply(black_box(&offer), None).unwrap())
    });
    let _ = zswap_state.try_apply(&offer, None).unwrap();
    zswap_local_state = zswap_local_state.watch_for(&keys.coin_public_key(), &coin);
    zswap_local_state = zswap_local_state.apply(&keys, &offer).unwrap();
    let qc = zswap_local_state.coins.iter().next().unwrap().1;
    let (_, input) = zswap_local_state.spend(&mut rng, &keys, &qc, None).unwrap();
    let offer = Offer {
        inputs: vec![input].into(),
        outputs: storage::storage::Array::new(),
        transient: storage::storage::Array::new(),
        deltas: vec![Delta {
            token_type: coin.type_,
            value: coin.value.try_into().expect("coin out of bounds"),
        }]
        .into(),
    };

    c.bench_function("ledger::proving", |b| {
        b.iter(|| {
            sync_prove(&offer, rng.split(), &resolver, &resolver).unwrap();
        })
    });

    let offer = sync_prove(&offer, rng.split(), &resolver, &resolver).unwrap();

    c.bench_function("ledger::validation", |b| {
        b.iter(|| {
            black_box(&offer).well_formed(0).unwrap();
        })
    });
}

pub fn zswap_local(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0x42);
    let mut zswap_state = ZswapLocalState::<InMemoryDB>::new();
    let keys = SecretKeys::from_rng_seed(&mut rng);
    let resolver = ZswapResolver(
        MidnightDataProvider::new(
            data_provider::FetchMode::Synchronous,
            data_provider::OutputMode::Log,
            ZSWAP_EXPECTED_FILES.to_owned(),
        )
        .unwrap(),
    );

    const CLAIM_AMOUNT: u128 = 5000000000;
    let coin = CoinInfo::new(&mut rng, CLAIM_AMOUNT, Default::default());
    let out = ZswapOutput::new(&mut rng, &coin, None, &keys.coin_public_key(), None).unwrap();

    let offer = Offer {
        inputs: storage::storage::Array::new(),
        outputs: vec![out].into(),
        transient: storage::storage::Array::new(),
        deltas: vec![Delta {
            token_type: Default::default(),
            value: -(CLAIM_AMOUNT as i128),
        }]
        .into(),
    };
    let offer = sync_prove(&offer, rng.split(), &resolver, &resolver).unwrap();

    c.bench_function("local::application", |b| {
        b.iter(|| {
            zswap_state = zswap_state
                .apply(black_box(&keys), black_box(&offer))
                .unwrap();
        })
    });
}

criterion_group!(
    name = benchmarking;
    config = Criterion::default().sample_size(10);
    targets = zswap_local, zswap_ledger);
criterion_main!(benchmarking);
