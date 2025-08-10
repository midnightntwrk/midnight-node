// .papi/descriptors/src/common.ts
var table = new Uint8Array(128);
for (let i = 0; i < 64; i++) table[i < 26 ? i + 65 : i < 52 ? i + 71 : i < 62 ? i - 4 : i * 4 - 205] = i;
var toBinary = (base64) => {
  const n = base64.length, bytes = new Uint8Array((n - Number(base64[n - 1] === "=") - Number(base64[n - 2] === "=")) * 3 / 4 | 0);
  for (let i2 = 0, j = 0; i2 < n; ) {
    const c0 = table[base64.charCodeAt(i2++)], c1 = table[base64.charCodeAt(i2++)];
    const c2 = table[base64.charCodeAt(i2++)], c3 = table[base64.charCodeAt(i2++)];
    bytes[j++] = c0 << 2 | c1 >> 4;
    bytes[j++] = c1 << 4 | c2 >> 2;
    bytes[j++] = c2 << 6 | c3;
  }
  return bytes;
};

// .papi/descriptors/src/undeployed.ts
var descriptorValues = import("./descriptors-CQRCDBVM.mjs").then((module) => module["Undeployed"]);
var metadataTypes = import("./metadataTypes-NMKH3ABE.mjs").then(
  (module) => toBinary("default" in module ? module.default : module)
);
var asset = {};
var getMetadata = () => import("./undeployed_metadata-IAHYEDYX.mjs").then(
  (module) => toBinary("default" in module ? module.default : module)
);
var genesis = void 0;
var _allDescriptors = { descriptors: descriptorValues, metadataTypes, asset, getMetadata, genesis };
var undeployed_default = _allDescriptors;

// .papi/descriptors/src/common-types.ts
import { _Enum } from "polkadot-api";
var DigestItem = _Enum;
var Phase = _Enum;
var DispatchClass = _Enum;
var TokenError = _Enum;
var ArithmeticError = _Enum;
var TransactionalError = _Enum;
var GrandpaEvent = _Enum;
var BalanceStatus = _Enum;
var PreimageEvent = _Enum;
var SessionEvent = _Enum;
var GrandpaStoredState = _Enum;
var BalancesTypesReasons = _Enum;
var PreimageOldRequestStatus = _Enum;
var PreimagesBounded = _Enum;
var DispatchRawOrigin = _Enum;
var GrandpaEquivocation = _Enum;
var MultiAddress = _Enum;
var BalancesAdjustmentDirection = _Enum;
var TransactionValidityUnknownTransaction = _Enum;
var TransactionValidityTransactionSource = _Enum;
var MmrPrimitivesError = _Enum;

// .papi/descriptors/src/index.ts
var metadatas = {};
var getMetadata2 = async (codeHash) => {
  try {
    return await metadatas[codeHash].getMetadata();
  } catch {
  }
  return null;
};
export {
  ArithmeticError,
  BalanceStatus,
  BalancesAdjustmentDirection,
  BalancesTypesReasons,
  DigestItem,
  DispatchClass,
  DispatchRawOrigin,
  GrandpaEquivocation,
  GrandpaEvent,
  GrandpaStoredState,
  MmrPrimitivesError,
  MultiAddress,
  Phase,
  PreimageEvent,
  PreimageOldRequestStatus,
  PreimagesBounded,
  SessionEvent,
  TokenError,
  TransactionValidityTransactionSource,
  TransactionValidityUnknownTransaction,
  TransactionalError,
  getMetadata2 as getMetadata,
  undeployed_default as undeployed
};
