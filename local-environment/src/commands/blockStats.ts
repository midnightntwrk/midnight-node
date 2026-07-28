// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { existsSync, writeFileSync } from "fs";
import path from "path";
import { globSync } from "glob";
import type { ApiPromise, WsProvider } from "@polkadot/api";
import type { Header } from "@polkadot/types/interfaces";
import { u8aToString } from "@polkadot/util";

import { createApi, disconnectApi } from "../lib/runtimeUpgradeUtils";
import { discoverValidatorEndpoints } from "../lib/discoverValidators";
import { NodeEndpoint } from "../lib/waitForFinality";

const AURA_ENGINE_ID = "aura";
const ZERO_HASH = `0x${"0".repeat(64)}`;
const DEFAULT_MC_EPOCH_MS = 60_000; // Cardano epoch duration on local-env
const DEFAULT_SC_SLOTS_PER_EPOCH = 5; // local-env sidechain slotsPerEpoch

export interface BlockStatsOptions {
  /** Number of finalized blocks to collect before summarizing. */
  blocks: number;
  /** Optional explicit endpoint list; replaces compose-file discovery. */
  nodeOverrides?: NodeEndpoint[];
  /** Where to write the JSON summary. Defaults to cwd. */
  outFile?: string;
  /** Sidechain slots per epoch (overrides on-chain derivation). */
  scSlotsPerEpoch?: number;
  /** Cardano epoch duration in ms (overrides default 60000). */
  mcEpochMs?: number;
}

/** One collected finalized block. */
interface BlockRecord {
  height: number;
  hash: string;
  slot: bigint;
  timestampMs: number;
  scEpoch: number;
  mcEpoch: number | null;
  sessionIndex: number;
  /** author account derived by polkadot.js from the aura digest (null if none). */
  authorAccount: string | null;
  /**
   * Expected author = session.validators.at(PARENT)[slot % len]. Read at the
   * parent because the author claims its slot from the parent's end-state; this
   * is boundary-safe (the first block of a session is authored by the previous
   * committee, whose set is still active in the parent state).
   */
  expectedAccount: string | null;
  /** derived author === expected. */
  authorMatches: boolean;
  /** aura authorities (hex) in on-chain order at this block (committee AURA walks). */
  committeeAuraKeys: string[];
  /** session.validators (accounts) at this block, index-aligned with the aura keys. */
  committeeAccounts: string[];
}

/** Timing anchors used to map a block to sidechain / Cardano epochs. */
interface EpochConfig {
  scSlotsPerEpoch: number;
  /** null when getStatus was unavailable → mc epoch mapping disabled. */
  mc: { epochNow: number; epochStartMs: number; epochMs: number } | null;
}

export async function blockStats(
  network: string | undefined,
  options: BlockStatsOptions,
): Promise<void> {
  const endpoints =
    options.nodeOverrides && options.nodeOverrides.length > 0
      ? options.nodeOverrides
      : discoverFromNetwork(network);

  const wsEndpoints = endpoints.map((e) => ({ name: e.name, url: toWs(e.url) }));
  console.log(
    `📊 block-stats: collecting ${options.blocks} finalized blocks from ` +
      `${wsEndpoints.map((e) => e.name).join(", ")}`,
  );

  const primary = wsEndpoints[0];
  const others = wsEndpoints.slice(1);

  const { api, provider } = await createApi(primary.url);
  const otherApis: { name: string; api: ApiPromise }[] = [];
  for (const e of others) {
    const conn = await createApi(e.url);
    otherApis.push({ name: e.name, api: conn.api });
  }

  try {
    const epochCfg = await resolveEpochConfig(api, provider, options);
    await wiringSanityCheck(api, provider);

    const records: BlockRecord[] = [];
    await collectFinalized(api, options.blocks, records, epochCfg);

    const crossNode = await checkCrossNodeAgreement(otherApis, records);
    const identity = await buildIdentityMap(provider, records);
    const committees = await fetchCommittees(provider, records, identity);
    const ariadne = await fetchAriadneByMcEpoch(provider, records, identity);

    const report = buildReport(
      records,
      crossNode,
      identity,
      committees,
      ariadne,
      wsEndpoints.map((e) => e.name),
    );

    const outFile =
      options.outFile ??
      path.resolve(
        process.cwd(),
        `block-stats-${records[0]?.height ?? 0}-${
          records[records.length - 1]?.height ?? 0
        }.json`,
      );
    writeFileSync(outFile, JSON.stringify(report, jsonReplacer, 2));

    printSummary(report, identity);
    console.log(`\n📄 Full summary written to ${outFile}`);

    if (!report.assertions.allPassed) {
      throw new Error(
        "block-stats assertions FAILED — see the summary above. " +
          "This indicates a regression in block authorship or session/committee plumbing.",
      );
    }
  } finally {
    await Promise.all(otherApis.map((o) => disconnectApi(o.api)));
    await disconnectApi(api);
  }
}

/** Anchor sidechain slotsPerEpoch and the Cardano epoch clock from getStatus. */
async function resolveEpochConfig(
  api: ApiPromise,
  provider: WsProvider,
  options: BlockStatsOptions,
): Promise<EpochConfig> {
  const mcEpochMs = options.mcEpochMs ?? DEFAULT_MC_EPOCH_MS;
  let scSlotsPerEpoch = options.scSlotsPerEpoch ?? DEFAULT_SC_SLOTS_PER_EPOCH;
  let mc: EpochConfig["mc"] = null;

  try {
    const status = (await provider.send("sidechain_getStatus", [])) as {
      sidechain?: { epoch?: number; slot?: number };
      mainchain?: { epoch?: number; nextEpochTimestamp?: number };
    };
    const scEpoch = status?.sidechain?.epoch;
    const scSlot = status?.sidechain?.slot;
    if (
      options.scSlotsPerEpoch === undefined &&
      scEpoch &&
      scSlot &&
      scEpoch > 0
    ) {
      // epoch = floor(slot / slotsPerEpoch) ⇒ slot/epoch ≈ slotsPerEpoch.
      scSlotsPerEpoch = Math.round(scSlot / scEpoch);
    }
    const mcEpochNow = status?.mainchain?.epoch;
    const mcNextTs = status?.mainchain?.nextEpochTimestamp;
    if (mcEpochNow !== undefined && mcNextTs !== undefined) {
      mc = {
        epochNow: mcEpochNow,
        epochStartMs: mcNextTs - mcEpochMs,
        epochMs: mcEpochMs,
      };
    }
  } catch {
    // getStatus unavailable → mc mapping disabled, sc slotsPerEpoch falls back.
  }

  void api;
  console.log(
    `🕒 epoch config: sidechain slotsPerEpoch=${scSlotsPerEpoch}` +
      (mc
        ? `, Cardano epoch=${mc.epochMs}ms (anchor mc epoch ${mc.epochNow})`
        : `, Cardano epoch mapping unavailable`),
  );
  return { scSlotsPerEpoch, mc };
}

/**
 * One-time cross-check that the pallet_session committee (post #1800 migration)
 * agrees with the sidechain committee-management view. Best-effort.
 */
async function wiringSanityCheck(
  api: ApiPromise,
  provider: WsProvider,
): Promise<void> {
  const auras = await api.query.aura.authorities();
  const validators = await api.query.session.validators();
  const sessionIndex = (await api.query.session.currentIndex()).toString();
  const auraLen = (auras as unknown as { length: number }).length;
  const valLen = (validators as unknown as { length: number }).length;

  console.log(
    `🔌 wiring: session #${sessionIndex}, ` +
      `aura.authorities=${auraLen}, session.validators=${valLen}`,
  );
  if (auraLen !== valLen) {
    console.warn(
      `⚠️  aura.authorities (${auraLen}) != session.validators (${valLen}). ` +
        `These must stay index-aligned for slot→author mapping to hold.`,
    );
  }

  try {
    const scStatus = (await provider.send("sidechain_getStatus", [])) as {
      sidechain?: { epoch?: number };
    };
    const scEpoch = scStatus?.sidechain?.epoch;
    if (scEpoch === undefined) return;
    const committee = (await provider.send("sidechain_getEpochCommittee", [
      scEpoch,
    ])) as { committee?: unknown[] };
    const members = committee?.committee ?? [];
    console.log(
      `🔌 wiring: sidechain epoch ${scEpoch} committee size ${members.length} ` +
        `(expected == aura.authorities ${auraLen})`,
    );
  } catch (err) {
    console.warn(
      `⚠️  sidechain committee cross-check skipped: ${(err as Error).message}`,
    );
  }
}

/**
 * Collect `target` contiguous finalized blocks, most-recent-ending-now. Reads
 * existing history immediately; streams forward only if the chain is younger.
 */
async function collectFinalized(
  api: ApiPromise,
  target: number,
  out: BlockRecord[],
  epochCfg: EpochConfig,
): Promise<void> {
  const finalizedHash = await api.rpc.chain.getFinalizedHead();
  const head = (await api.rpc.chain.getHeader(finalizedHash)).number.toNumber();

  // Skip genesis (#0 has no aura digest). Anchor the window on the current head.
  const start = Math.max(1, head - target + 1);
  if (head >= start) {
    console.log(
      `  reading ${head - start + 1} existing finalized block(s) #${start}..#${head}`,
    );
  }
  for (let h = start; h <= head && out.length < target; h++) {
    await recordHeight(api, h, out, target, epochCfg);
  }

  if (out.length >= target) return;

  console.log(
    `  chain has only ${out.length} block(s); waiting for ` +
      `${target - out.length} more finalized block(s)…`,
  );
  await streamForward(api, target, out, head + 1, epochCfg);
}

async function recordHeight(
  api: ApiPromise,
  height: number,
  out: BlockRecord[],
  target: number,
  epochCfg: EpochConfig,
): Promise<void> {
  const hash = await api.rpc.chain.getBlockHash(height);
  const header = await api.rpc.chain.getHeader(hash);
  const rec = await recordFor(api, header, epochCfg);
  out.push(rec);
  if (out.length % 100 === 0 || out.length === target) {
    console.log(
      `  … ${out.length}/${target} blocks (height ${height}, sc epoch ${rec.scEpoch})`,
    );
  }
}

async function streamForward(
  api: ApiPromise,
  target: number,
  out: BlockRecord[],
  next: number,
  epochCfg: EpochConfig,
): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    let unsub: (() => void) | undefined;
    let high = -1;
    let cursor = next;
    let running = false;

    const pump = async () => {
      if (running) return;
      running = true;
      try {
        while (cursor <= high && out.length < target) {
          await recordHeight(api, cursor, out, target, epochCfg);
          cursor += 1;
        }
        if (out.length >= target) {
          if (unsub) unsub();
          resolve();
          return;
        }
      } catch (err) {
        if (unsub) unsub();
        reject(err);
        return;
      } finally {
        running = false;
      }
      if (out.length < target && cursor <= high) void pump();
    };

    api.rpc.chain
      .subscribeFinalizedHeads((h: Header) => {
        const n = h.number.toNumber();
        if (n > high) high = n;
        void pump();
      })
      .then((u) => {
        unsub = u;
      })
      .catch(reject);
  });
}

async function recordFor(
  api: ApiPromise,
  header: Header,
  epochCfg: EpochConfig,
): Promise<BlockRecord> {
  const hash = header.hash.toHex();
  const height = header.number.toNumber();
  const slot = extractAuraSlot(api, header);

  const apiAt = await api.at(hash);
  const sessionIndex = Number(
    (await apiAt.query.session.currentIndex()).toString(),
  );
  const timestampMs = Number((await apiAt.query.timestamp.now()).toString());

  const validators = await apiAt.query.session.validators();
  const committeeAccounts = (
    validators as unknown as { toString: () => string }[]
  ).map((v) => v.toString());
  const authorities = await apiAt.query.aura.authorities();
  const committeeAuraKeys = (
    authorities as unknown as { toHex: () => string }[]
  ).map((a) => a.toHex());

  // Expected author from PARENT state (boundary-safe — see BlockRecord docs).
  let expectedAccount: string | null = null;
  if (slot !== null) {
    const apiAtParent = await api.at(header.parentHash);
    const parentValidators = await apiAtParent.query.session.validators();
    const parentArr = parentValidators as unknown as {
      length: number;
      toString: () => string;
    }[];
    if (parentArr.length > 0) {
      const idx = Number(slot % BigInt(parentArr.length));
      expectedAccount = parentArr[idx]?.toString() ?? null;
    }
  }

  const extended = await api.derive.chain.getHeader(header.hash);
  const authorAccount = extended?.author ? extended.author.toString() : null;

  const scEpoch =
    slot !== null ? Number(slot / BigInt(epochCfg.scSlotsPerEpoch)) : -1;
  const mcEpoch = epochCfg.mc ? mcEpochOf(timestampMs, epochCfg.mc) : null;

  return {
    height,
    hash,
    slot: slot ?? -1n,
    timestampMs,
    scEpoch,
    mcEpoch,
    sessionIndex,
    authorAccount,
    expectedAccount,
    authorMatches:
      authorAccount !== null &&
      expectedAccount !== null &&
      authorAccount === expectedAccount,
    committeeAuraKeys,
    committeeAccounts,
  };
}

function mcEpochOf(
  ts: number,
  mc: { epochNow: number; epochStartMs: number; epochMs: number },
): number {
  return mc.epochNow + Math.floor((ts - mc.epochStartMs) / mc.epochMs);
}

/** Read the aura slot (u64 LE) out of the block's PreRuntime digest. */
function extractAuraSlot(api: ApiPromise, header: Header): bigint | null {
  for (const log of header.digest.logs) {
    if (!log.isPreRuntime) continue;
    const [engine, data] = log.asPreRuntime;
    if (u8aToString(engine.toU8a()) !== AURA_ENGINE_ID) continue;
    return api.createType("u64", data.toU8a(true)).toBigInt();
  }
  return null;
}

/**
 * For each collected height, confirm every other node reports the same block
 * hash. A zero/absent hash is normal tip lag (block not imported yet), not a
 * fork — only a conflicting non-zero hash counts as a disagreement.
 */
async function checkCrossNodeAgreement(
  others: { name: string; api: ApiPromise }[],
  records: BlockRecord[],
): Promise<{ checked: number; disagreements: string[]; lagging: number }> {
  const disagreements: string[] = [];
  let lagging = 0;
  if (others.length === 0) return { checked: 0, disagreements, lagging };

  for (const rec of records) {
    for (const o of others) {
      const h = (await o.api.rpc.chain.getBlockHash(rec.height)).toHex();
      if (h === rec.hash) continue;
      if (h === ZERO_HASH) {
        lagging += 1;
        continue;
      }
      disagreements.push(
        `height ${rec.height}: ${o.name}=${h} != primary=${rec.hash}`,
      );
    }
  }
  return { checked: records.length, disagreements, lagging };
}

// ---------------------------------------------------------------------------
// Identity mapping: one canonical id per validator, joining the three views
// (per-block author account, aura authority key, sidechain public key).
// ---------------------------------------------------------------------------

interface Identity {
  /** account (SS58) -> aura key (hex) */
  accountToAura: Map<string, string>;
  /** aura key (hex) -> sidechain pub key (hex) */
  auraToSidechain: Map<string, string>;
  /** sidechain pub key (hex) -> aura key (hex) */
  sidechainToAura: Map<string, string>;
  /** canonical id (aura key) -> short human label (V1, V2, …) */
  label: Map<string, string>;
}

/** account or aura key -> canonical id (aura key when known, else the input). */
function canonAura(identity: Identity, account: string): string {
  return identity.accountToAura.get(account) ?? account;
}

async function buildIdentityMap(
  provider: WsProvider,
  records: BlockRecord[],
): Promise<Identity> {
  const accountToAura = new Map<string, string>();
  // aura.authorities and session.validators are index-aligned per block.
  for (const r of records) {
    const n = Math.min(r.committeeAccounts.length, r.committeeAuraKeys.length);
    for (let i = 0; i < n; i++) {
      accountToAura.set(r.committeeAccounts[i], r.committeeAuraKeys[i]);
    }
  }

  const auraToSidechain = new Map<string, string>();
  const sidechainToAura = new Map<string, string>();
  const mcEpochs = distinct(
    records.map((r) => r.mcEpoch).filter((e): e is number => e !== null),
  );
  for (const mcEpoch of mcEpochs) {
    const params = await getAriadne(provider, mcEpoch);
    for (const c of params?.permissionedCandidates ?? []) {
      const aura = c.keys?.aura;
      const sc = c.sidechainPublicKey;
      if (aura && sc) {
        auraToSidechain.set(aura, sc);
        sidechainToAura.set(sc, aura);
      }
    }
  }

  // Stable labels by first appearance in the collected stream.
  const label = new Map<string, string>();
  let n = 0;
  for (const r of records) {
    for (const aura of r.committeeAuraKeys) {
      if (!label.has(aura)) label.set(aura, `V${++n}`);
    }
  }
  return { accountToAura, auraToSidechain, sidechainToAura, label };
}

// ---------------------------------------------------------------------------
// Committee (seat) reconstruction per sidechain epoch and Ariadne params per
// Cardano epoch (for transition auto-detection).
// ---------------------------------------------------------------------------

interface AriadneParams {
  dParameter?: {
    numPermissionedCandidates?: number;
    numRegisteredCandidates?: number;
  };
  permissionedCandidates?: {
    sidechainPublicKey?: string;
    keys?: { aura?: string };
  }[];
}

async function getAriadne(
  provider: WsProvider,
  mcEpoch: number,
): Promise<AriadneParams | null> {
  // systemParameters_* sources the D-parameter from the on-chain pallet (the
  // effective value used for selection); sidechain_getAriadneParameters returns
  // the raw mainchain UTxO which can read (0,0) even while selection uses (P,R).
  try {
    return (await provider.send("systemParameters_getAriadneParameters", [
      mcEpoch,
    ])) as AriadneParams;
  } catch {
    return null;
  }
}

/** Per sidechain epoch: the committee the RPC reports (canonical ids, ordered). */
interface CommitteeInfo {
  scEpoch: number;
  /** canonical ids (aura keys) with repeats, from sidechain_getEpochCommittee */
  rpcMembers: string[] | null;
  /** canonical ids observed in aura.authorities for this epoch (with repeats) */
  auraMembers: string[];
}

async function fetchCommittees(
  provider: WsProvider,
  records: BlockRecord[],
  identity: Identity,
): Promise<Map<number, CommitteeInfo>> {
  // aura authorities are constant within a sidechain epoch — snapshot per epoch.
  const auraByEpoch = new Map<number, string[]>();
  for (const r of records) {
    if (r.scEpoch >= 0 && !auraByEpoch.has(r.scEpoch)) {
      auraByEpoch.set(r.scEpoch, r.committeeAuraKeys);
    }
  }

  const out = new Map<number, CommitteeInfo>();
  for (const [scEpoch, auraKeys] of auraByEpoch) {
    let rpcMembers: string[] | null = null;
    try {
      const resp = (await provider.send("sidechain_getEpochCommittee", [
        scEpoch,
      ])) as { committee?: { sidechainPubKey?: string }[] };
      rpcMembers = (resp?.committee ?? []).map((m) => {
        const sc = m.sidechainPubKey ?? "";
        return identity.sidechainToAura.get(sc) ?? sc;
      });
    } catch {
      rpcMembers = null;
    }
    out.set(scEpoch, {
      scEpoch,
      rpcMembers,
      auraMembers: auraKeys, // already canonical (aura keys)
    });
  }
  return out;
}

/** Per Cardano epoch: permissioned candidate set (canonical ids) + dParam. */
interface McEpochInfo {
  mcEpoch: number;
  permissioned: string[]; // canonical ids (aura keys)
  dParam: { p: number; r: number } | null;
}

async function fetchAriadneByMcEpoch(
  provider: WsProvider,
  records: BlockRecord[],
  identity: Identity,
): Promise<Map<number, McEpochInfo>> {
  const out = new Map<number, McEpochInfo>();
  const mcEpochs = distinct(
    records.map((r) => r.mcEpoch).filter((e): e is number => e !== null),
  );
  for (const mcEpoch of mcEpochs) {
    const params = await getAriadne(provider, mcEpoch);
    const permissioned = (params?.permissionedCandidates ?? [])
      .map((c) => c.keys?.aura ?? c.sidechainPublicKey ?? "")
      .filter((x) => x !== "");
    const d = params?.dParameter;
    out.set(mcEpoch, {
      mcEpoch,
      permissioned,
      dParam:
        d && d.numPermissionedCandidates !== undefined
          ? { p: d.numPermissionedCandidates, r: d.numRegisteredCandidates ?? 0 }
          : null,
    });
    void identity;
  }
  return out;
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

interface McEpochRow {
  mcEpoch: number;
  scEpochs: number[];
  dParam: { p: number; r: number } | null;
  permissionedCount: number;
  /** canonical id -> seats this Cardano epoch (summed over its sc epochs) */
  seats: Record<string, number>;
  /** canonical id -> blocks observed this Cardano epoch */
  blocks: Record<string, number>;
  /** canonical id -> cumulative average blocks per Cardano epoch up to here */
  cumulativeAvg: Record<string, number>;
}

/** Compact per-block row for offline analysis (author vs the seat-holder). */
interface PerBlock {
  height: number;
  slot: string;
  scEpoch: number;
  mcEpoch: number | null;
  /** actual author (label) */
  author: string;
  /** who the current epoch's committee assigns this slot to = committee[slot%len] */
  seat: string;
  /** author != seat → first block of the epoch, produced by the previous committee */
  boundary: boolean;
}

interface Report {
  collectedBlocks: number;
  firstHeight: number;
  lastHeight: number;
  nodes: string[];
  perBlock: PerBlock[];
  scSlotsPerEpoch: number;
  authorTotals: Record<string, number>; // canonical id -> blocks
  transition: {
    detected: boolean;
    atMcEpoch: number | null;
    removed: string[];
    added: string[];
  };
  mcEpochRows: McEpochRow[];
  crossNode: { checked: number; disagreements: string[]; lagging: number };
  slotGaps: {
    afterHeight: number;
    missedSlots: number;
    missed: { slot: string; validator: string }[];
  }[];
  assertions: {
    allPassed: boolean;
    authorMatchesExpected: boolean;
    seatAllocationMatchesRpc: boolean;
    sessionsMonotonic: boolean;
    crossNodeAgrees: boolean;
    noStall: boolean;
    details: string[];
  };
}

function buildReport(
  records: BlockRecord[],
  crossNode: { checked: number; disagreements: string[]; lagging: number },
  identity: Identity,
  committees: Map<number, CommitteeInfo>,
  ariadne: Map<number, McEpochInfo>,
  nodes: string[],
): Report {
  const details: string[] = [];

  // 1. Strict per-slot author == expected(parent-state).
  const checkable = records.filter(
    (r) => r.slot >= 0n && r.expectedAccount !== null && r.authorAccount !== null,
  );
  const mismatches = checkable.filter((r) => !r.authorMatches);
  const authorMatchesExpected = mismatches.length === 0;
  if (checkable.length === 0) {
    details.push("no blocks had a resolvable author+expected — check skipped");
  } else if (!authorMatchesExpected) {
    const e = mismatches[0];
    details.push(
      `${mismatches.length}/${checkable.length} block(s) author != expected(parent), ` +
        `e.g. #${e.height} slot ${e.slot} sc-epoch ${e.scEpoch}: ` +
        `author ${short(e.authorAccount)} != expected ${short(e.expectedAccount)}`,
    );
  }

  // 2. Seat allocation: aura authorities multiset == getEpochCommittee multiset.
  let seatAllocationMatchesRpc = true;
  let seatChecked = 0;
  for (const info of committees.values()) {
    if (!info.rpcMembers) continue;
    seatChecked++;
    if (!multisetEqual(info.auraMembers, info.rpcMembers)) {
      seatAllocationMatchesRpc = false;
      details.push(
        `sc-epoch ${info.scEpoch}: aura authorities != getEpochCommittee ` +
          `(aura ${fmtMultiset(info.auraMembers, identity)} vs rpc ${fmtMultiset(info.rpcMembers, identity)})`,
      );
      break;
    }
  }
  if (seatChecked === 0) {
    details.push("getEpochCommittee unavailable — seat-allocation check skipped");
  }

  // 3. session index monotonic.
  let sessionsMonotonic = true;
  for (let i = 1; i < records.length; i++) {
    if (records[i].sessionIndex < records[i - 1].sessionIndex) {
      sessionsMonotonic = false;
      details.push(
        `session index went backwards at #${records[i].height}: ` +
          `${records[i - 1].sessionIndex} -> ${records[i].sessionIndex}`,
      );
      break;
    }
  }

  // 4. cross-node agreement.
  const crossNodeAgrees = crossNode.disagreements.length === 0;
  if (!crossNodeAgrees) {
    details.push(
      `cross-node disagreement on ${crossNode.disagreements.length} height(s), ` +
        `e.g. ${crossNode.disagreements[0]}`,
    );
  }
  if (crossNode.lagging > 0) {
    details.push(
      `${crossNode.lagging} peer/height probe(s) not imported at query time (tip lag, ignored)`,
    );
  }

  // 5. no stall + missed slots (with the validator assigned to each empty slot,
  //    resolved from that slot's epoch committee = committee[slot % len]).
  const spe = deriveSlotsPerEpoch(records);
  const slotGaps: Report["slotGaps"] = [];
  let heightGap = false;
  for (let i = 1; i < records.length; i++) {
    if (records[i].height !== records[i - 1].height + 1) heightGap = true;
    const prev = records[i - 1].slot;
    const cur = records[i].slot;
    if (prev >= 0n && cur > prev + 1n) {
      const missed: { slot: string; validator: string }[] = [];
      for (let s = prev + 1n; s < cur; s++) {
        const scEpoch = Number(s / BigInt(spe));
        const members = committees.get(scEpoch)?.auraMembers ?? [];
        const who =
          members.length > 0
            ? idLabel(identity, members[Number(s % BigInt(members.length))])
            : "?";
        missed.push({ slot: s.toString(), validator: who });
      }
      slotGaps.push({
        afterHeight: records[i - 1].height,
        missedSlots: Number(cur - prev - 1n),
        missed,
      });
    }
  }
  const noStall = !heightGap;
  if (heightGap) details.push("finalized height sequence has gaps (stall/reorg?)");

  const transition = detectTransition(ariadne, identity);
  const mcEpochRows = buildMcEpochRows(records, committees, ariadne, identity);
  const authorTotals: Record<string, number> = {};
  for (const r of records) {
    if (!r.authorAccount) continue;
    const id = canonAura(identity, r.authorAccount);
    authorTotals[id] = (authorTotals[id] ?? 0) + 1;
  }

  const allPassed =
    authorMatchesExpected &&
    seatAllocationMatchesRpc &&
    sessionsMonotonic &&
    crossNodeAgrees &&
    noStall;

  const perBlock: PerBlock[] = records.map((r) => {
    const len = r.committeeAuraKeys.length;
    const seat =
      r.slot >= 0n && len > 0
        ? r.committeeAuraKeys[Number(r.slot % BigInt(len))]
        : null;
    const authorId = r.authorAccount ? canonAura(identity, r.authorAccount) : null;
    return {
      height: r.height,
      slot: r.slot.toString(),
      scEpoch: r.scEpoch,
      mcEpoch: r.mcEpoch,
      author: authorId ? idLabel(identity, authorId) : "<none>",
      seat: seat ? idLabel(identity, seat) : "<none>",
      boundary: authorId !== null && seat !== null && authorId !== seat,
    };
  });

  return {
    collectedBlocks: records.length,
    firstHeight: records[0]?.height ?? 0,
    lastHeight: records[records.length - 1]?.height ?? 0,
    nodes,
    perBlock,
    scSlotsPerEpoch: records[0] ? deriveSlotsPerEpoch(records) : 0,
    authorTotals,
    transition,
    mcEpochRows,
    crossNode,
    slotGaps,
    assertions: {
      allPassed,
      authorMatchesExpected,
      seatAllocationMatchesRpc,
      sessionsMonotonic,
      crossNodeAgrees,
      noStall,
      details,
    },
  };
}

function deriveSlotsPerEpoch(records: BlockRecord[]): number {
  // Reconstruct from any block: slot / scEpoch is ~slotsPerEpoch; use exact
  // block-count of the most common full sc epoch instead → robust fallback.
  const r = records.find((x) => x.slot >= 0n && x.scEpoch >= 0);
  if (!r || r.scEpoch === 0) return DEFAULT_SC_SLOTS_PER_EPOCH;
  return Math.max(1, Math.round(Number(r.slot) / r.scEpoch));
}

/** Detect a change in the permissioned candidate set across Cardano epochs. */
function detectTransition(
  ariadne: Map<number, McEpochInfo>,
  identity: Identity,
): Report["transition"] {
  const epochs = [...ariadne.values()].sort((a, b) => a.mcEpoch - b.mcEpoch);
  for (let i = 1; i < epochs.length; i++) {
    const prev = new Set(epochs[i - 1].permissioned);
    const cur = new Set(epochs[i].permissioned);
    const removed = [...prev].filter((x) => !cur.has(x));
    const added = [...cur].filter((x) => !prev.has(x));
    if (removed.length > 0 || added.length > 0) {
      return {
        detected: true,
        atMcEpoch: epochs[i].mcEpoch,
        removed: removed.map((x) => idLabel(identity, x)),
        added: added.map((x) => idLabel(identity, x)),
      };
    }
  }
  return { detected: false, atMcEpoch: null, removed: [], added: [] };
}

function buildMcEpochRows(
  records: BlockRecord[],
  committees: Map<number, CommitteeInfo>,
  ariadne: Map<number, McEpochInfo>,
  identity: Identity,
): McEpochRow[] {
  const byMc = new Map<number, BlockRecord[]>();
  for (const r of records) {
    if (r.mcEpoch === null) continue;
    let arr = byMc.get(r.mcEpoch);
    if (!arr) {
      arr = [];
      byMc.set(r.mcEpoch, arr);
    }
    arr.push(r);
  }
  // Attribute each block to the epoch whose committee PRODUCED it = the parent
  // block's epoch. The first block of a sidechain epoch is produced by the
  // previous committee (rotation runs during that block), so crediting it to
  // the parent's epoch makes blocks reconcile with seats; any residual
  // blocks < seats is then a genuinely missed slot. (Records are height-contiguous,
  // so records[i-1] is always the true parent.)
  const blocksByProdMc = new Map<number, Record<string, number>>();
  for (let i = 0; i < records.length; i++) {
    const r = records[i];
    if (!r.authorAccount) continue;
    const parent = i > 0 ? records[i - 1] : r;
    const prodMc = parent.mcEpoch;
    if (prodMc === null) continue;
    const id = canonAura(identity, r.authorAccount);
    const m = blocksByProdMc.get(prodMc) ?? {};
    m[id] = (m[id] ?? 0) + 1;
    blocksByProdMc.set(prodMc, m);
  }

  const cumulativeBlocks: Record<string, number> = {};
  const rows: McEpochRow[] = [];
  let epochCount = 0;

  for (const mcEpoch of [...byMc.keys()].sort((a, b) => a - b)) {
    epochCount++;
    const recs = byMc.get(mcEpoch) ?? [];
    const scEpochs = distinct(recs.map((r) => r.scEpoch)).sort((a, b) => a - b);

    const seats: Record<string, number> = {};
    for (const scEpoch of scEpochs) {
      const info = committees.get(scEpoch);
      for (const id of info?.auraMembers ?? []) {
        seats[id] = (seats[id] ?? 0) + 1;
      }
    }

    const blocks = blocksByProdMc.get(mcEpoch) ?? {};
    for (const id of Object.keys(blocks)) {
      cumulativeBlocks[id] = (cumulativeBlocks[id] ?? 0) + blocks[id];
    }

    const cumulativeAvg: Record<string, number> = {};
    for (const id of Object.keys(cumulativeBlocks)) {
      cumulativeAvg[id] = cumulativeBlocks[id] / epochCount;
    }

    const info = ariadne.get(mcEpoch);
    rows.push({
      mcEpoch,
      scEpochs,
      dParam: info?.dParam ?? null,
      permissionedCount: info?.permissioned.length ?? 0,
      seats,
      blocks,
      cumulativeAvg,
    });
  }
  return rows;
}

// ---------------------------------------------------------------------------
// Printing
// ---------------------------------------------------------------------------

function printSummary(report: Report, identity: Identity): void {
  const a = report.assertions;
  console.log(
    `\n===== block-stats: ${report.collectedBlocks} blocks ` +
      `(#${report.firstHeight}..#${report.lastHeight}) =====`,
  );

  // Legend
  const ids = Object.keys(report.authorTotals).sort(
    (x, y) => report.authorTotals[y] - report.authorTotals[x],
  );
  console.log("\nValidators (label = aura… / sidechain… / blocks):");
  for (const id of ids) {
    console.log(
      `  ${idLabel(identity, id)}  ${short(id)}  ` +
        `${short(identity.auraToSidechain.get(id) ?? "-")}  ` +
        `${report.authorTotals[id]}`,
    );
  }

  if (report.transition.detected) {
    console.log(
      `\n🔀 Candidate-set change detected at Cardano epoch ${report.transition.atMcEpoch}: ` +
        `removed [${report.transition.removed.join(", ") || "—"}] ` +
        `added [${report.transition.added.join(", ") || "—"}]`,
    );
  } else {
    console.log("\n(no permissioned candidate-set change observed in window)");
  }

  // Per-Cardano-epoch seats/blocks + cumulative average
  console.log(
    "\nPer Cardano epoch — seats | blocks | cumulative avg blocks/epoch " +
      "(blocks credited to the producing committee, so blocks<seats ⇒ a missed slot):",
  );
  for (const row of report.mcEpochRows) {
    const members = distinct([
      ...Object.keys(row.seats),
      ...Object.keys(row.blocks),
    ]).sort((x, y) => idLabel(identity, x).localeCompare(idLabel(identity, y)));
    const dp = row.dParam ? `D(${row.dParam.p},${row.dParam.r})` : "D(?)";
    console.log(
      `  mc ${row.mcEpoch} [${dp}, N=${row.permissionedCount}, sc ${row.scEpochs.join(",")}]:`,
    );
    for (const id of members) {
      console.log(
        `      ${idLabel(identity, id).padEnd(4)} seats=${row.seats[id] ?? 0} ` +
          `blocks=${row.blocks[id] ?? 0} avg=${(row.cumulativeAvg[id] ?? 0).toFixed(2)}`,
      );
    }
  }

  if (report.slotGaps.length > 0) {
    const missed = report.slotGaps.reduce((s, g) => s + g.missedSlots, 0);
    console.log(
      `\nMissed slots: ${missed} across ${report.slotGaps.length} gap(s) ` +
        `(empty slots where the assigned validator did not produce):`,
    );
    for (const g of report.slotGaps) {
      const who = g.missed
        .map((m) => `${m.validator} @ slot ${m.slot}`)
        .join(", ");
      console.log(`  after #${g.afterHeight}: ${g.missedSlots} missed — ${who}`);
    }
  }

  console.log("\nAssertions:");
  line("per-slot author == expected (parent-state)", a.authorMatchesExpected);
  line("seat allocation == getEpochCommittee", a.seatAllocationMatchesRpc);
  line("session index monotonic", a.sessionsMonotonic);
  line("cross-node agreement", a.crossNodeAgrees);
  line("no finalized-height stall", a.noStall);
  for (const d of a.details) console.log(`    - ${d}`);

  console.log(`\n${a.allPassed ? "✅ ALL ASSERTIONS PASSED" : "❌ FAILURES"}`);
}

function line(label: string, ok: boolean): void {
  console.log(`  ${ok ? "✅" : "❌"} ${label}`);
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

function idLabel(identity: Identity, id: string): string {
  return identity.label.get(id) ?? short(id);
}

function short(s: string | null): string {
  if (!s) return "<none>";
  return s.length > 12 ? `${s.slice(0, 8)}…${s.slice(-4)}` : s;
}

function distinct<T>(xs: T[]): T[] {
  return [...new Set(xs)];
}

function multisetEqual(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  const count = new Map<string, number>();
  for (const x of a) count.set(x, (count.get(x) ?? 0) + 1);
  for (const x of b) {
    const c = count.get(x);
    if (!c) return false;
    count.set(x, c - 1);
  }
  return [...count.values()].every((c) => c === 0);
}

function fmtMultiset(xs: string[], identity: Identity): string {
  const count = new Map<string, number>();
  for (const x of xs) count.set(x, (count.get(x) ?? 0) + 1);
  return (
    "{" +
    [...count.entries()]
      .map(([id, c]) => `${idLabel(identity, id)}×${c}`)
      .join(",") +
    "}"
  );
}

function jsonReplacer(_key: string, value: unknown): unknown {
  return typeof value === "bigint" ? value.toString() : value;
}

function toWs(url: string): string {
  return url
    .replace(/^http:/, "ws:")
    .replace(/^https:/, "wss:")
    .replace("localhost", "127.0.0.1");
}

function discoverFromNetwork(network: string | undefined): NodeEndpoint[] {
  if (!network) {
    throw new Error(
      "block-stats requires either a <network> argument or one or more --node overrides",
    );
  }
  return discoverValidatorEndpoints(resolveComposeFile(network));
}

function resolveComposeFile(network: string): string {
  if (network === "local-env") {
    const composeFile = path.resolve(
      __dirname,
      "../networks/local-env/docker-compose.yml",
    );
    if (!existsSync(composeFile)) {
      throw new Error(`Compose file not found: ${composeFile}`);
    }
    return composeFile;
  }

  const searchPath = path.resolve(
    __dirname,
    "../networks",
    "well-known",
    network,
    "*.network.yaml",
  );
  const candidates = globSync(searchPath);
  if (candidates.length === 0) {
    throw new Error(
      `No compose file found for network '${network}' under well-known/`,
    );
  }
  const preferred = candidates.find(
    (p) => path.basename(p) === `${network}.network.yaml`,
  );
  return preferred ?? candidates[0];
}
