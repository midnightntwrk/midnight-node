#![deny(warnings)]
#![allow(unused_imports)]
#![allow(unused)]
use base_crypto::rng::SplittableRng;
use base_crypto::signatures::Signature;
use base_crypto::time::Timestamp;
use coin_structure::coin::UserAddress;
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use lazy_static::lazy_static;
use midnight_ledger::dust::{DustPublicKey, InitialNonce};
use midnight_ledger::prove::Resolver;
use midnight_ledger::semantics::{TransactionContext, TransactionResult};
use midnight_ledger::structure::{
    CNightGeneratesDustActionType, CNightGeneratesDustEvent, Intent, LedgerState,
    OutputInstructionUnshielded, ProofMarker, SystemTransaction, UnshieldedOffer, UtxoOutput,
    UtxoSpend,
};
use midnight_ledger::structure::{ClaimKind, Transaction};
#[cfg(feature = "proving")]
use midnight_ledger::test_utilities::well_formed_tx_builder;
use midnight_ledger::test_utilities::{TestState, test_resolver};
use midnight_ledger::verify::WellFormedStrictness;
use onchain_runtime::context::BlockContext;
use pprof::criterion::{Output, PProfProfiler};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng, seq::SliceRandom};
use std::alloc::GlobalAlloc;
use std::io::{Write, stdout};
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
use storage::DefaultHasher;
use storage::arena::{Sp, TCONSTRUCT};
use storage::backend::StorageBackendStats;
use storage::db::{DB, InMemoryDB, ParityDb};
use storage::storage::{
    DEFAULT_CACHE_SIZE, default_storage, set_default_storage, unsafe_drop_default_storage,
};
use transient_crypto::commitment::PedersenRandomness;
use zswap::keys::SecretKeys;

#[global_allocator]
static GLOBAL_ALLOC: Allocator<std::alloc::System> = Allocator(std::alloc::System);
static CURALLOC: AtomicU64 = AtomicU64::new(0);

fn pprint_bytes(n: u64) -> String {
    const KB: u64 = 1 << 10;
    const MB: u64 = 1 << 20;
    const GB: u64 = 1 << 30;
    match n {
        0..KB => format!("{n} B"),
        KB..MB => format!("{:.2} kiB", n as f64 / KB as f64),
        MB..GB => format!("{:.2} MiB", n as f64 / MB as f64),
        GB.. => format!("{:.2} GiB", n as f64 / GB as f64),
    }
}

fn cur_alloc() -> String {
    pprint_bytes(CURALLOC.load(std::sync::atomic::Ordering::SeqCst))
}

fn du(dir: PathBuf) -> std::io::Result<u64> {
    let mut frontier = vec![dir];
    let mut total = 0;
    while let Some(dir) = frontier.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            if meta.is_dir() {
                frontier.push(entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    Ok(total)
}

struct Allocator<A: GlobalAlloc>(A);

unsafe impl<A: GlobalAlloc> GlobalAlloc for Allocator<A> {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        CURALLOC
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |x| Some(x + layout.size() as u64),
            )
            .unwrap();
        unsafe { self.0.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        CURALLOC
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |x| Some(x - layout.size() as u64),
            )
            .unwrap();
        unsafe { self.0.dealloc(ptr, layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        CURALLOC
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |x| Some(x + layout.size() as u64),
            )
            .unwrap();
        unsafe { self.0.alloc_zeroed(layout) }
    }
}

lazy_static! {
    static ref RESOLVER: Resolver = test_resolver("");
}

pub fn rewards(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0x42);
    fn mk_rewards<R: Rng>(rng: &mut R, n: usize) -> SystemTransaction {
        let mut outputs = Vec::with_capacity(n);
        for _ in 0..n {
            outputs.push(OutputInstructionUnshielded {
                amount: rng.r#gen::<u32>() as u128,
                target_address: UserAddress(rng.r#gen()),
                nonce: rng.r#gen(),
            });
        }
        SystemTransaction::DistributeNight(ClaimKind::Reward, outputs)
    }
    let mut ledger_state: LedgerState<InMemoryDB> = LedgerState::new("local-test");
    ledger_state = ledger_state
        .apply_system_tx(
            &SystemTransaction::DistributeReserve(ledger_state.reserve_pool),
            Timestamp::from_secs(0),
        )
        .unwrap()
        .0;

    let mut group = c.benchmark_group("rewards");
    for size in [100, 200, 300, 400, 500] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let tx = mk_rewards(&mut rng, size);
            b.iter(|| {
                black_box(
                    ledger_state
                        .apply_system_tx(&tx, Timestamp::from_secs(0))
                        .unwrap(),
                )
            });
        });
    }
    group.finish();
}

#[cfg(feature = "proving")]
pub fn transaction_validation(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0x42);
    let ledger_state: LedgerState<InMemoryDB> = LedgerState::new("local-test");
    let sk = SecretKeys::from_rng_seed(&mut rng);
    let tx: Transaction<(), ProofMarker, PedersenRandomness, InMemoryDB> =
        well_formed_tx_builder(rng.split(), &sk, &RESOLVER).unwrap();

    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;

    c.bench_function("verification", |b| {
        b.iter(|| {
            black_box(&tx)
                .well_formed(&ledger_state, strictness, Timestamp::from_secs(0))
                .unwrap();
        })
    });
    let vtx = tx
        .well_formed(&ledger_state, strictness, Timestamp::from_secs(0))
        .unwrap();
    let context = TransactionContext {
        ref_state: LedgerState::new("local-test"),
        block_context: BlockContext::default(),
        whitelist: None,
    };

    c.bench_function("application", |b| {
        b.iter(|| black_box(ledger_state.apply(black_box(&vtx), black_box(&context))))
    });
}

pub fn create_dust(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0x42);
    fn mk_dust<R: Rng>(rng: &mut R, n: usize) -> SystemTransaction {
        let mut events = Vec::with_capacity(n);
        for _ in 0..n {
            events.push(CNightGeneratesDustEvent {
                value: rng.r#gen::<u32>() as u128,
                owner: DustPublicKey(rng.r#gen()),
                time: Timestamp::from_secs(0),
                action: CNightGeneratesDustActionType::Create,
                nonce: InitialNonce(rng.r#gen()),
            });
        }
        SystemTransaction::CNightGeneratesDustUpdate { events }
    }
    let ledger_state: LedgerState<InMemoryDB> = LedgerState::new("local-test");

    let mut group = c.benchmark_group("create-dust");
    for size in [100, 200, 300, 400] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let tx = mk_dust(&mut rng, size);
            b.iter(|| {
                black_box(
                    ledger_state
                        .apply_system_tx(&tx, Timestamp::from_secs(0))
                        .unwrap(),
                )
            });
        });
    }
    group.finish();
}

type TestDb = ParityDb;

fn mk_test_db() -> (PathBuf, storage::Storage<TestDb>) {
    let dir = tempfile::tempdir().unwrap().keep();
    let db = storage::Storage::new(
        //10,
        DEFAULT_CACHE_SIZE,
        ParityDb::<DefaultHasher>::open(&dir),
        //InMemoryDB::default(),
    );
    (dir, db)
}

pub fn night_transfer_by_utxo_set_size(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut rng = StdRng::seed_from_u64(0x42);
    let mut group = c.benchmark_group("night-transfer-by-utxo-set-size");
    let (dir, db) = mk_test_db();
    set_default_storage(|| db);
    let mut state = TestState::new(&mut rng);
    let mut reached_size = 0;
    for log_size in 10..=13 {
        let t0 = std::time::Instant::now();
        let mut swizzle_time = std::time::Duration::default();
        let mut gc_time = std::time::Duration::default();
        let size = 2u64.pow(log_size);
        state.mode = midnight_ledger::test_utilities::TestProcessingMode::ForceConstantTime;
        const PROGRESS_BAR_SEGMENTS: usize = 50;
        const BATCH_SIZE: u64 = 4;
        print!("[{}]", " ".repeat(PROGRESS_BAR_SEGMENTS));
        stdout().flush().unwrap();
        let start = reached_size >> BATCH_SIZE;
        let end = size >> BATCH_SIZE;
        let mut last_str_len = 0usize;
        let mut culled = 0usize;
        for i in start..end {
            let ta = std::time::Instant::now();
            rt.block_on(state.give_fee_token(&mut rng, 1 << BATCH_SIZE));
            let tb = std::time::Instant::now();
            state.swizzle_to_db();
            swizzle_time += tb.elapsed();
            let tc = std::time::Instant::now();
            state.swizzle_to_db();
            culled += default_storage::<TestDb>()
                .with_backend(|b| b.gc(std::time::Duration::from_millis(500)));
            gc_time += tc.elapsed();
            let frac = (i + 1 - start) as f64 / (end - start) as f64;
            let segments = (frac * PROGRESS_BAR_SEGMENTS as f64).round() as usize;
            let string = format!(
                "[{}{}] ETA: {:?}, TPS: {}, size: {}, write time: {:?}, GC time: {:?}, culled: {culled}",
                "#".repeat(segments),
                " ".repeat(PROGRESS_BAR_SEGMENTS - segments),
                t0.elapsed().div_f64(frac).mul_f64(1.0 - frac),
                (1 << BATCH_SIZE) as f64 / ta.elapsed().as_secs_f64(),
                (i + 1) << BATCH_SIZE,
                swizzle_time / ((i + 1 - start) << BATCH_SIZE) as u32,
                gc_time / ((i + 1 - start) << BATCH_SIZE) as u32,
            );
            print!(
                "\r{string}{}",
                " ".repeat(last_str_len.saturating_sub(string.len()))
            );
            last_str_len = string.len();
            stdout().flush().unwrap();
        }
        println!();
        reached_size = size;
        let mut lstate = Sp::new(state.ledger.clone());
        let mut utxos = lstate.utxo.utxos.keys().collect::<Vec<_>>();
        utxos.shuffle(&mut rng);
        let txs = (0..256)
            .map(|_| {
                let utxo = utxos.pop().unwrap();
                let offer: UnshieldedOffer<Signature, TestDb> = UnshieldedOffer {
                    inputs: vec![UtxoSpend {
                        value: utxo.value,
                        owner: state.night_key.verifying_key(),
                        type_: utxo.type_,
                        intent_hash: utxo.intent_hash,
                        output_no: utxo.output_no,
                    }]
                    .into(),
                    outputs: vec![
                        UtxoOutput {
                            value: 100,
                            owner: UserAddress::from(state.night_key.verifying_key()),
                            type_: utxo.type_,
                        },
                        UtxoOutput {
                            value: utxo.value - 100,
                            owner: UserAddress::from(state.night_key.verifying_key()),
                            type_: utxo.type_,
                        },
                    ]
                    .into(),
                    signatures: vec![].into(),
                };
                let intent = Intent::new(
                    &mut rng,
                    Some(offer),
                    None,
                    vec![],
                    vec![],
                    vec![],
                    None,
                    state.time,
                );
                let tx =
                    Transaction::from_intents("local-test", [(1, intent)].into_iter().collect());
                rt.block_on(state.balance_tx(rng.split(), tx, &test_resolver("benchmarks")))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let context = state.context().block_context;
        let key = lstate.as_typed_key();
        let t1 = std::time::Instant::now();
        println!(
            "Took {:?} ({:?} per; of which {:?} writes) to init {size} entries, with {} allocated ({} DB)",
            t1 - t0,
            (t1 - t0) / ((end - start) << BATCH_SIZE) as u32,
            swizzle_time / ((end - start) << BATCH_SIZE) as u32,
            cur_alloc(),
            pprint_bytes(du(dir.clone()).unwrap()),
        );
        lstate.persist();
        drop(lstate);
        state.swizzle_to_db();
        default_storage::<TestDb>().with_backend(|b| {
            b.flush_all_changes_to_db();
        });
        let mut runs = 0;
        let mut total_construct_time =
            std::collections::HashMap::<&'static str, (usize, std::time::Duration)>::new();
        #[derive(Copy, Clone, PartialEq, Eq, Debug)]
        enum Mode {
            Verify,
            ReadVerify,
            Compute,
            Write,
            ReadCompute,
        };
        let time = state.time;
        for mode in [
            Mode::Verify,
            Mode::ReadVerify,
            Mode::Compute,
            Mode::Write,
            Mode::ReadCompute,
        ] {
            group.bench_with_input(
                BenchmarkId::new(format!("{mode:?}"), log_size),
                &log_size,
                |b, _| {
                    b.iter_custom(|i| {
                        let mut dt = Default::default();
                        for _ in 0..i {
                            let pre_cache =
                                default_storage::<TestDb>().with_backend(|b| b.get_stats());
                            let tconstruct0 =
                                TCONSTRUCT.lock().unwrap().clone().unwrap_or_default();
                            let state = black_box(default_storage().get_lazy(&key)).unwrap();
                            let context = TransactionContext {
                                ref_state: state.deref().clone(),
                                block_context: context.clone(),
                                whitelist: None,
                            };
                            let txs = txs
                                .choose_multiple(&mut rng, 1 << BATCH_SIZE)
                                .cloned()
                                .collect::<Vec<_>>();
                            let mut t0 = std::time::Instant::now();
                            let strictness = WellFormedStrictness::default();
                            let vtxs = txs
                                .iter()
                                .map(|tx| tx.well_formed(&*state, strictness, time).unwrap())
                                .collect::<Vec<_>>();
                            if mode == Mode::ReadVerify {
                                dt += t0.elapsed();
                                continue;
                            } else if mode == Mode::Verify {
                                t0 = std::time::Instant::now();
                                let _ = txs
                                    .iter()
                                    .map(|tx| tx.well_formed(&*state, strictness, time).unwrap())
                                    .collect::<Vec<_>>();
                                dt += t0.elapsed();
                                continue;
                            }
                            t0 = std::time::Instant::now();
                            let (s, _) = black_box(
                                state.batch_apply_all_or_nothing(black_box(&vtxs), &context),
                            )
                            .unwrap();
                            // Re-run it, it's guaranteed to be in cache now!
                            if mode == Mode::Compute {
                                t0 = std::time::Instant::now();
                                let _ = black_box(
                                    state.batch_apply_all_or_nothing(black_box(&vtxs), &context),
                                )
                                .unwrap();
                            } else if mode == Mode::Write {
                                let mut s = Sp::new(s);
                                t0 = std::time::Instant::now();
                                s.persist();
                                default_storage::<TestDb>()
                                    .with_backend(|b| b.flush_all_changes_to_db());
                                s.unpersist();
                            }
                            dt += t0.elapsed();
                            let tconstruct1 =
                                TCONSTRUCT.lock().unwrap().clone().unwrap_or_default();
                            if mode == Mode::ReadCompute {
                                for (k, v) in tconstruct1 {
                                    total_construct_time.entry(k).or_default().0 +=
                                        v.0 - tconstruct0.get(k).copied().unwrap_or_default().0;
                                    total_construct_time.entry(k).or_default().1 +=
                                        v.1 - tconstruct0.get(k).copied().unwrap_or_default().1;
                                }
                                let post_cache =
                                    default_storage::<TestDb>().with_backend(|b| b.get_stats());
                                runs += 1;
                            }
                        }
                        dt
                    });
                },
            );
        }
        let mut total_construct_time = total_construct_time
            .into_iter()
            .map(|(k, (n, d))| (k, (n / runs as usize, d / runs)))
            .collect::<Vec<_>>();
        total_construct_time.sort_by_key(|v| v.1.1);
        println!(
            "Construct time: {:?}",
            total_construct_time
                .iter()
                .map(|(_, (_, d))| d)
                .sum::<Duration>()
        );
        let lstate = default_storage::<TestDb>().get_lazy(&key).unwrap();
        lstate.unpersist();
        drop(lstate);
        drop(key);
        drop(utxos);
        drop(txs);
        println!("After dropping all data, {} remains allocated", cur_alloc());
    }
    group.finish();
}

pub fn create_and_destroy_dust(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(0x42);
    fn mk_tx<R: Rng>(rng: &mut R, n: usize) -> SystemTransaction {
        let mut events = Vec::with_capacity(n);
        let mut owners = Vec::with_capacity(n);
        for _ in 0..n {
            if owners.is_empty() || rng.gen_bool(0.5) {
                let key = DustPublicKey(rng.r#gen());
                let amt = rng.r#gen::<u32>() as u128;
                owners.push((key, amt));
                events.push(CNightGeneratesDustEvent {
                    value: amt,
                    owner: key,
                    time: Timestamp::from_secs(0),
                    action: CNightGeneratesDustActionType::Create,
                    nonce: InitialNonce(rng.r#gen()),
                });
            } else {
                let key: DustPublicKey;
                let amt: u128;
                loop {
                    let idx = rng.gen_range(0..owners.len());
                    let (k, max_amt) = owners[idx];
                    if max_amt == 0 {
                        continue;
                    }
                    key = k;
                    amt = rng.gen_range(0..max_amt);
                    owners[idx] = (key, max_amt - amt);
                    break;
                }
                events.push(CNightGeneratesDustEvent {
                    value: amt,
                    owner: key,
                    time: Timestamp::from_secs(0),
                    action: CNightGeneratesDustActionType::Destroy,
                    nonce: InitialNonce(rng.r#gen()),
                })
            }
        }
        SystemTransaction::CNightGeneratesDustUpdate { events }
    }
    let ledger_state: LedgerState<InMemoryDB> = LedgerState::new("local-test");
    let mut group = c.benchmark_group("create-and-destroy-dust");
    for size in [100, 200, 300, 400] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let tx = mk_tx(&mut rng, size);
            b.iter(|| {
                black_box(
                    ledger_state
                        .apply_system_tx(&tx, Timestamp::from_secs(0))
                        .unwrap(),
                )
            });
        });
    }
    group.finish();
}

#[cfg(feature = "proving")]
criterion_group!(
    name = benchmarking;
    config = Criterion::default().sample_size(10);
    targets = transaction_validation
);
criterion_group!(system_tx, rewards, create_dust, create_and_destroy_dust);
criterion_group!(
    name = night;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = night_transfer_by_utxo_set_size
);
#[cfg(feature = "proving")]
criterion_main!(benchmarking, system_tx, night);
#[cfg(not(feature = "proving"))]
criterion_main!(system_tx, night);
