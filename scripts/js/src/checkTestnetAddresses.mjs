#!/usr/bin/env node
import { ApiPromise, WsProvider } from '@polkadot/api'
import jsonrpc from '@polkadot/types/interfaces/jsonrpc'
import fs from 'fs';

async function main() {
    const api = await ApiPromise.create({
        provider: new WsProvider('wss://rpc.testnet-02.midnight.network'),
        rpc: { ...jsonrpc },
    })
    const queryResponse = await api.query.sessionCommitteeManagement.mainChainScriptsConfiguration()
    const file = JSON.parse(fs.readFileSync('/addresses.json', 'utf8'));
    const normalizeHex = (hex) => `0x${hex.toLowerCase().replace('0x', '')}`;
    const committeeCandidateAddress = file.addresses.CommitteeCandidateValidator;
    const dParameterPolicy = normalizeHex(file.mintingPolicies.DParameterPolicy);
    const permissionedCandidatesPolicy = normalizeHex(file.mintingPolicies.PermissionedCandidatesPolicy);
    if (queryResponse.committeeCandidateAddress.toHuman() !== committeeCandidateAddress) {
        console.error(`Committee Candidate Address is incorrect. Expected: ${queryResponse.committeeCandidateAddress}, Actual: ${committeeCandidateAddress}`);
        throw new Error('Committee Candidate Address is incorrect');
    }
    if (queryResponse.dParameterPolicyId.toHuman() !== dParameterPolicy) {
        console.error(`dParameterPolicyId is incorrect. Expected: ${queryResponse.dParameterPolicyId}, Actual: ${dParameterPolicy}`);
        throw new Error('dParameterPolicyId is incorrect');
    }
    if (queryResponse.permissionedCandidatesPolicyId.toHuman() !== permissionedCandidatesPolicy) {
        console.error(`permissionedCandidatesPolicyId is incorrect. Expected: ${queryResponse.permissionedCandidatesPolicyId}, Actual: ${permissionedCandidatesPolicy}`);
        throw new Error('permissionedCandidatesPolicyId is incorrect');
    }
    await api.disconnect()
}

main().catch((error) => {
    console.error('Check failed:', error);
    process.exit(1);
});
